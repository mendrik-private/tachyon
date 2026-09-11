use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::{AsyncReadExt as _, FutureExt as _, future::Shared};
use gpui::{
    App, Asset as _, AssetLogger, Context, ImageAssetLoader, ImageCache, ImageCacheError,
    ImageCacheItem, RenderImage, Resource, Task, Window, hash,
    http_client::{
        AsyncBody, HttpClient, HttpRequestExt as _, Method, RedirectPolicy, Request, StatusCode,
        http,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MAX_CONCURRENT_LOADS: usize = 4;
const MAX_CACHE_ENTRIES: usize = 256;
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: usize = 16_384;
const MAX_IMAGE_PIXELS: usize = 64 * 1024 * 1024;
const DISK_CACHE_BYTES: u64 = 512 * 1024 * 1024;

type ImageDimensions = Arc<Mutex<(u64, HashMap<u64, (u32, u32)>)>>;
type PreparedImageTask = Shared<Task<Result<PreparedImage, String>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedImage {
    path: PathBuf,
    dimensions: (u32, u32),
}

enum CacheEntry {
    Fetching(PreparedImageTask),
    Decoding(ImageCacheItem),
    Failed(ImageCacheError),
}

pub(crate) struct BoundedImageCache {
    max_bytes: usize,
    total_bytes: usize,
    recency: Vec<u64>,
    weights: HashMap<u64, usize>,
    entries: HashMap<u64, CacheEntry>,
    disk: DiskImageCache,
    dimensions: ImageDimensions,
}

impl BoundedImageCache {
    pub(crate) fn new(max_bytes: usize, cx: &mut Context<Self>) -> Self {
        cx.on_release(|cache, cx| {
            for (_, entry) in std::mem::take(&mut cache.entries) {
                if let CacheEntry::Decoding(mut item) = entry
                    && let Some(Ok(image)) = item.get()
                {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        Self {
            max_bytes,
            total_bytes: 0,
            recency: Vec::new(),
            weights: HashMap::new(),
            entries: HashMap::new(),
            disk: DiskImageCache::for_current_user(DISK_CACHE_BYTES),
            dimensions: Arc::new(Mutex::new((0, HashMap::new()))),
        }
    }

    pub(crate) fn dimensions(&self) -> ImageDimensions {
        self.dimensions.clone()
    }

    /// Advances document-wide image decoding through the same bounded queue
    /// used by visible images. Completed and failed resources leave the list;
    /// cache notifications schedule another frame for the remaining work.
    pub(crate) fn prefetch(
        &mut self,
        resources: &mut Vec<Resource>,
        window: &mut Window,
        cx: &mut App,
    ) {
        resources.retain(|resource| self.load(resource, window, cx).is_none());
    }

    pub(crate) fn retry_failed(&mut self, resource: &Resource) -> bool {
        let key = hash(resource);
        let failed = match self.entries.get_mut(&key) {
            Some(CacheEntry::Failed(_)) => true,
            Some(CacheEntry::Decoding(item)) => matches!(item.get(), Some(Err(_))),
            _ => false,
        };
        if failed {
            self.entries.remove(&key);
            self.recency.retain(|candidate| *candidate != key);
            self.total_bytes = self
                .total_bytes
                .saturating_sub(self.weights.remove(&key).unwrap_or(0));
        }
        failed
    }

    fn touch(&mut self, key: u64) {
        self.recency.retain(|candidate| *candidate != key);
        self.recency.insert(0, key);
    }

    fn decoded_bytes(image: &RenderImage) -> usize {
        (0..image.frame_count())
            .filter_map(|frame| image.as_bytes(frame).map(<[u8]>::len))
            .sum()
    }

    fn record_weight(&mut self, key: u64, bytes: usize) {
        if self.weights.contains_key(&key) {
            return;
        }
        self.weights.insert(key, bytes);
        self.total_bytes = self.total_bytes.saturating_add(bytes);
    }

    fn evict(&mut self, key: u64, window: &mut Window, cx: &mut App) {
        self.recency.retain(|candidate| *candidate != key);
        self.total_bytes = self
            .total_bytes
            .saturating_sub(self.weights.remove(&key).unwrap_or(0));
        if let Some(CacheEntry::Decoding(mut item)) = self.entries.remove(&key)
            && let Some(Ok(image)) = item.get()
        {
            cx.drop_image(image, Some(window));
        }
    }

    fn trim(&mut self, protected: u64, window: &mut Window, cx: &mut App) {
        while self.total_bytes > self.max_bytes {
            let Some(index) = self
                .recency
                .iter()
                .rposition(|candidate| *candidate != protected)
            else {
                break;
            };
            let key = self.recency.remove(index);
            self.evict(key, window, cx);
        }
    }

    fn loading_count(&mut self) -> usize {
        self.entries
            .values_mut()
            .map(|entry| match entry {
                CacheEntry::Fetching(_) => 1,
                CacheEntry::Decoding(item) => usize::from(item.get().is_none()),
                CacheEntry::Failed(_) => 0,
            })
            .sum()
    }

    fn schedule_notification<T>(task: Shared<Task<T>>, window: &mut Window, cx: &mut App)
    where
        T: Clone + 'static,
    {
        let entity = window.current_view();
        window
            .spawn(cx, async move |cx| {
                let _ = task.await;
                cx.on_next_frame(move |_, cx| cx.notify(entity));
            })
            .detach();
    }

    fn start_decode(&mut self, key: u64, resource: Resource, window: &mut Window, cx: &mut App) {
        let future = AssetLogger::<ImageAssetLoader>::load(resource, cx);
        let task = cx.background_executor().spawn(future).shared();
        self.entries.insert(
            key,
            CacheEntry::Decoding(ImageCacheItem::Loading(task.clone())),
        );
        Self::schedule_notification(task, window, cx);
    }
}

impl ImageCache for BoundedImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        let fetched = match self.entries.get_mut(&key) {
            Some(CacheEntry::Fetching(task)) => task.clone().now_or_never(),
            _ => None,
        };
        if let Some(result) = fetched {
            match result {
                Ok(prepared) => {
                    if let Ok(mut dimensions) = self.dimensions.lock()
                        && dimensions.1.insert(key, prepared.dimensions)
                            != Some(prepared.dimensions)
                    {
                        dimensions.0 = dimensions.0.wrapping_add(1);
                    }
                    self.start_decode(key, Resource::Path(prepared.path.into()), window, cx);
                    return None;
                }
                Err(error) => {
                    let error = ImageCacheError::Asset(error.into());
                    self.entries.insert(key, CacheEntry::Failed(error.clone()));
                    return Some(Err(error));
                }
            }
        }

        if let Some(CacheEntry::Decoding(item)) = self.entries.get_mut(&key)
            && let Some(result) = item.get()
        {
            if let Ok(image) = &result {
                let bytes = Self::decoded_bytes(image);
                if bytes > self.max_bytes {
                    self.evict(key, window, cx);
                    return Some(Err(ImageCacheError::Asset(
                        format!(
                            "decoded image uses {bytes} bytes, exceeding the {} byte limit",
                            self.max_bytes
                        )
                        .into(),
                    )));
                }
                self.record_weight(key, bytes);
            }
            self.touch(key);
            self.trim(key, window, cx);
            return Some(result);
        }
        if let Some(CacheEntry::Failed(error)) = self.entries.get(&key) {
            return Some(Err(error.clone()));
        }
        if self.entries.contains_key(&key) || self.loading_count() >= MAX_CONCURRENT_LOADS {
            return None;
        }
        while self.entries.len() >= MAX_CACHE_ENTRIES {
            let Some(oldest) = self.recency.last().copied() else {
                break;
            };
            self.evict(oldest, window, cx);
        }
        self.touch(key);

        let task = match resource {
            Resource::Uri(uri) => {
                let uri = uri.as_ref().to_owned();
                let disk = self.disk.clone();
                let client = cx.http_client();
                cx.background_executor()
                    .spawn_dedicated(move |_| async move { disk.resolve(&uri, client).await })
                    .shared()
            }
            Resource::Path(path) => {
                let path = path.to_path_buf();
                cx.background_executor()
                    .spawn_dedicated(move |_| async move { prepare_local_image(path) })
                    .shared()
            }
            Resource::Embedded(_) => {
                self.start_decode(key, resource.clone(), window, cx);
                return None;
            }
        };
        self.entries.insert(key, CacheEntry::Fetching(task.clone()));
        Self::schedule_notification(task, window, cx);
        None
    }
}

#[derive(Clone)]
struct DiskImageCache {
    directory: PathBuf,
    max_bytes: u64,
    filesystem_lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiskCacheRecord {
    etag: Option<String>,
    last_modified: Option<String>,
    size: u64,
    last_access_unix_ms: u128,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

impl DiskImageCache {
    fn for_current_user(max_bytes: u64) -> Self {
        let directory = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir)
            .join("mineral-markdown/images");
        Self {
            directory,
            max_bytes,
            filesystem_lock: Arc::new(Mutex::new(())),
        }
    }

    #[cfg(test)]
    fn in_directory(directory: PathBuf, max_bytes: u64) -> Self {
        Self {
            directory,
            max_bytes,
            filesystem_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn resolve(
        &self,
        url: &str,
        client: Arc<dyn HttpClient>,
    ) -> Result<PreparedImage, String> {
        let cached = self.read_record(url)?;
        let mut builder = Request::builder()
            .uri(url)
            .method(Method::GET)
            .follow_redirects(RedirectPolicy::FollowAll);
        if let Some((record, _)) = cached.as_ref() {
            if let Some(etag) = record.etag.as_deref() {
                builder = builder.header(http::header::IF_NONE_MATCH, etag);
            }
            if let Some(last_modified) = record.last_modified.as_deref() {
                builder = builder.header(http::header::IF_MODIFIED_SINCE, last_modified);
            }
        }
        let request = builder
            .body(AsyncBody::empty())
            .map_err(|error| error.to_string())?;
        let response = client.send(request).await;
        let mut response = match response {
            Ok(response) => response,
            Err(error) => {
                if let Some((record, path)) = cached {
                    self.touch_record(url, &record)?;
                    return Ok(prepared_from_record(record, path));
                }
                return Err(format!("image download failed: {error}"));
            }
        };

        if response.status() == StatusCode::NOT_MODIFIED
            && let Some((record, path)) = cached
        {
            self.touch_record(url, &record)?;
            return Ok(prepared_from_record(record, path));
        }
        if !response.status().is_success() {
            if response.status().is_server_error()
                && let Some((record, path)) = cached
            {
                self.touch_record(url, &record)?;
                return Ok(prepared_from_record(record, path));
            }
            return Err(format!("image request returned HTTP {}", response.status()));
        }
        if response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|length| length > MAX_DOWNLOAD_BYTES)
        {
            return Err(format!(
                "image download exceeds the {MAX_DOWNLOAD_BYTES} byte limit"
            ));
        }
        let etag = response
            .headers()
            .get(http::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let last_modified = response
            .headers()
            .get(http::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let mut bytes = Vec::new();
        response
            .body_mut()
            .take(MAX_DOWNLOAD_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| format!("image body read failed: {error}"))?;
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(format!(
                "image download exceeds the {MAX_DOWNLOAD_BYTES} byte limit"
            ));
        }
        self.store(url, &bytes, etag, last_modified)
    }

    fn read_record(&self, url: &str) -> Result<Option<(DiskCacheRecord, PathBuf)>, String> {
        let _guard = self
            .filesystem_lock
            .lock()
            .map_err(|_| "image cache lock was poisoned".to_owned())?;
        let (data_path, record_path) = self.paths(url);
        let bytes = match fs::read(&record_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read image cache metadata: {error}")),
        };
        let mut record = match serde_json::from_slice::<DiskCacheRecord>(&bytes) {
            Ok(record) => record,
            Err(_) => {
                let _ = fs::remove_file(&record_path);
                let _ = fs::remove_file(&data_path);
                return Ok(None);
            }
        };
        if !data_path.is_file() {
            let _ = fs::remove_file(record_path);
            return Ok(None);
        }
        if record.width == 0 || record.height == 0 {
            let image = fs::read(&data_path)
                .map_err(|error| format!("cannot read cached image: {error}"))?;
            let (width, height) = validate_image_dimensions(&image)?;
            record.width = width;
            record.height = height;
            write_atomic(
                &record_path,
                &serde_json::to_vec(&record).map_err(|error| error.to_string())?,
            )?;
        }
        Ok(Some((record, data_path)))
    }

    fn touch_record(&self, url: &str, record: &DiskCacheRecord) -> Result<(), String> {
        let _guard = self
            .filesystem_lock
            .lock()
            .map_err(|_| "image cache lock was poisoned".to_owned())?;
        let mut record = record.clone();
        record.last_access_unix_ms = now_unix_ms();
        let (_, record_path) = self.paths(url);
        write_atomic(
            &record_path,
            &serde_json::to_vec(&record).map_err(|error| error.to_string())?,
        )
    }

    fn store(
        &self,
        url: &str,
        bytes: &[u8],
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<PreparedImage, String> {
        let _guard = self
            .filesystem_lock
            .lock()
            .map_err(|_| "image cache lock was poisoned".to_owned())?;
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("cannot create image cache: {error}"))?;
        let (data_path, record_path) = self.paths(url);
        let (width, height) = validate_image_dimensions(bytes)?;
        write_atomic(&data_path, bytes)?;
        let record = DiskCacheRecord {
            etag,
            last_modified,
            size: bytes.len() as u64,
            last_access_unix_ms: now_unix_ms(),
            width,
            height,
        };
        write_atomic(
            &record_path,
            &serde_json::to_vec(&record).map_err(|error| error.to_string())?,
        )?;
        self.trim_locked()?;
        Ok(PreparedImage {
            path: data_path,
            dimensions: (width, height),
        })
    }

    fn trim_locked(&self) -> Result<(), String> {
        let mut records = fs::read_dir(&self.directory)
            .map_err(|error| format!("cannot inspect image cache: {error}"))?
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|value| value.to_str()) == Some("json"))
                    .then(|| fs::read(&path).ok())
                    .flatten()
                    .and_then(|bytes| serde_json::from_slice::<DiskCacheRecord>(&bytes).ok())
                    .map(|record| (path, record))
            })
            .collect::<Vec<_>>();
        let mut total = records.iter().map(|(_, record)| record.size).sum::<u64>();
        records.sort_by_key(|(_, record)| record.last_access_unix_ms);
        for (record_path, record) in records {
            if total <= self.max_bytes {
                break;
            }
            let Some(key) = record_path.file_stem().and_then(|key| key.to_str()) else {
                continue;
            };
            let data_path = self.directory.join(format!("{key}.image"));
            let _ = fs::remove_file(data_path);
            let _ = fs::remove_file(record_path);
            total = total.saturating_sub(record.size);
        }
        Ok(())
    }

    fn paths(&self, url: &str) -> (PathBuf, PathBuf) {
        let key = cache_key(url);
        (
            self.directory.join(format!("{key}.image")),
            self.directory.join(format!("{key}.json")),
        )
    }
}

fn validate_image_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    let (width, height) = imagesize::blob_size(bytes)
        .map(|dimensions| (dimensions.width, dimensions.height))
        .or_else(|_| svg_dimensions(bytes))?;
    let pixels = width.saturating_mul(height);
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION || pixels > MAX_IMAGE_PIXELS {
        return Err(format!(
            "image dimensions {}x{} exceed the decoded-image limit",
            width, height
        ));
    }
    Ok((width as u32, height as u32))
}

fn prepared_from_record(record: DiskCacheRecord, path: PathBuf) -> PreparedImage {
    PreparedImage {
        path,
        dimensions: (record.width, record.height),
    }
}

fn prepare_local_image(path: PathBuf) -> Result<PreparedImage, String> {
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("cannot inspect local image {}: {error}", path.display()))?;
    if metadata.len() > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "local image exceeds the {MAX_DOWNLOAD_BYTES} byte limit"
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read local image {}: {error}", path.display()))?;
    let dimensions = validate_image_dimensions(&bytes)?;
    Ok(PreparedImage { path, dimensions })
}

fn svg_dimensions(bytes: &[u8]) -> Result<(usize, usize), String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|error| format!("image dimensions could not be read: {error}"))?;
    let document = roxmltree::Document::parse(source)
        .map_err(|error| format!("SVG header is malformed: {error}"))?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("image dimensions could not be read".into());
    }
    let explicit = root
        .attribute("width")
        .and_then(svg_length)
        .zip(root.attribute("height").and_then(svg_length));
    let view_box = root.attribute("viewBox").and_then(|view_box| {
        let values = view_box
            .split(|character: char| character.is_ascii_whitespace() || character == ',')
            .filter(|value| !value.is_empty())
            .filter_map(|value| value.parse::<f64>().ok())
            .collect::<Vec<_>>();
        (values.len() == 4).then_some((values[2], values[3]))
    });
    let (width, height) = explicit.or(view_box).unwrap_or((300., 150.));
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("SVG dimensions must be finite and positive".into());
    }
    Ok((width.ceil() as usize, height.ceil() as usize))
}

fn svg_length(value: &str) -> Option<f64> {
    let value = value.trim();
    let numeric_end = value
        .find(|character: char| !(character.is_ascii_digit() || matches!(character, '.' | '-')))
        .unwrap_or(value.len());
    let unit = value[numeric_end..].trim();
    if !matches!(unit, "" | "px" | "pt" | "pc" | "mm" | "cm" | "in") {
        return None;
    }
    let value = value[..numeric_end].parse::<f64>().ok()?;
    let pixels_per_unit = match unit {
        "pt" => 96. / 72.,
        "pc" => 16.,
        "mm" => 96. / 25.4,
        "cm" => 96. / 2.54,
        "in" => 96.,
        _ => 1.,
    };
    Some(value * pixels_per_unit)
}

fn cache_key(url: &str) -> String {
    Sha256::digest(url.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}-{}", std::process::id(), now_unix_ms()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    // Cache tests exercise image bytes, not the format of packaging artwork.
    const TEST_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><rect width="128" height="128" fill="green"/></svg>"#;

    struct TestHttpClient {
        not_modified: bool,
        saw_etag: Arc<AtomicBool>,
        user_agent: http::HeaderValue,
    }

    impl HttpClient for TestHttpClient {
        fn user_agent(&self) -> Option<&http::HeaderValue> {
            Some(&self.user_agent)
        }

        fn proxy(&self) -> Option<&gpui::http_client::Url> {
            None
        }

        fn send(
            &self,
            request: Request<AsyncBody>,
        ) -> futures::future::BoxFuture<
            'static,
            gpui::http_client::Result<gpui::http_client::Response<AsyncBody>>,
        > {
            self.saw_etag.store(
                request.headers().contains_key(http::header::IF_NONE_MATCH),
                Ordering::SeqCst,
            );
            let not_modified = self.not_modified;
            Box::pin(async move {
                if not_modified {
                    Ok(gpui::http_client::Response::builder()
                        .status(StatusCode::NOT_MODIFIED)
                        .body(AsyncBody::empty())?)
                } else {
                    Err(gpui::http_client::anyhow!("offline"))
                }
            })
        }
    }

    #[test]
    fn recency_is_unique_and_most_recent_first() {
        let mut cache = BoundedImageCache {
            max_bytes: 10,
            total_bytes: 0,
            recency: Vec::new(),
            weights: HashMap::new(),
            entries: HashMap::new(),
            disk: DiskImageCache::in_directory(PathBuf::new(), 10),
            dimensions: Arc::new(Mutex::new((0, HashMap::new()))),
        };
        cache.touch(1);
        cache.touch(2);
        cache.touch(1);
        assert_eq!(cache.recency, vec![1, 2]);
    }

    #[test]
    fn disk_cache_evicts_the_oldest_record() {
        let directory = std::env::temp_dir().join(format!(
            "mineral-image-cache-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let cache = DiskImageCache::in_directory(directory.clone(), 5);
        fs::create_dir_all(&directory).expect("cache fixture");
        let old_url = "https://example.test/old.png";
        let new_url = "https://example.test/new.png";
        let (old_data, old_record) = cache.paths(old_url);
        let (new_data, new_record) = cache.paths(new_url);
        fs::write(&old_data, b"1234").expect("old data");
        fs::write(&new_data, b"5678").expect("new data");
        fs::write(
            &old_record,
            serde_json::to_vec(&DiskCacheRecord {
                etag: None,
                last_modified: None,
                size: 4,
                last_access_unix_ms: 1,
                width: 1,
                height: 1,
            })
            .expect("old record"),
        )
        .expect("old metadata");
        fs::write(
            &new_record,
            serde_json::to_vec(&DiskCacheRecord {
                etag: None,
                last_modified: None,
                size: 4,
                last_access_unix_ms: 2,
                width: 1,
                height: 1,
            })
            .expect("new record"),
        )
        .expect("new metadata");

        cache.trim_locked().expect("trim cache");
        assert!(!old_data.exists());
        assert!(new_data.exists());
        fs::remove_dir_all(directory).expect("cleanup isolated cache fixture");
    }

    #[test]
    fn cache_keys_are_stable_and_url_specific() {
        assert_eq!(cache_key("a"), cache_key("a"));
        assert_ne!(cache_key("a"), cache_key("b"));
        assert_eq!(cache_key("a").len(), 64);
    }

    #[test]
    fn image_headers_are_checked_before_decode() {
        let dimensions = validate_image_dimensions(TEST_SVG).expect("bounded SVG dimensions");
        assert_eq!(dimensions, (128, 128));
    }

    #[test]
    fn cached_images_revalidate_and_remain_available_offline() {
        let directory = std::env::temp_dir().join(format!(
            "mineral-image-revalidation-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let cache = DiskImageCache::in_directory(directory.clone(), DISK_CACHE_BYTES);
        let url = "https://example.test/cached.png";
        let prepared = cache
            .store(url, TEST_SVG, Some("\"v1\"".into()), None)
            .expect("seed cache");
        let saw_etag = Arc::new(AtomicBool::new(false));
        let revalidated = futures::executor::block_on(cache.resolve(
            url,
            Arc::new(TestHttpClient {
                not_modified: true,
                saw_etag: saw_etag.clone(),
                user_agent: http::HeaderValue::from_static("test"),
            }),
        ))
        .expect("304 uses cache");
        assert_eq!(revalidated, prepared);
        assert!(saw_etag.load(Ordering::SeqCst));

        let offline = futures::executor::block_on(cache.resolve(
            url,
            Arc::new(TestHttpClient {
                not_modified: false,
                saw_etag,
                user_agent: http::HeaderValue::from_static("test"),
            }),
        ))
        .expect("offline uses cache");
        assert_eq!(offline, prepared);
        fs::remove_dir_all(directory).expect("cleanup isolated cache fixture");
    }

    #[test]
    fn explicit_retry_only_drops_failed_entries() {
        let dimensions = Arc::new(Mutex::new((0, HashMap::new())));
        let mut cache = BoundedImageCache {
            max_bytes: 10,
            total_bytes: 0,
            recency: Vec::new(),
            weights: HashMap::new(),
            entries: HashMap::new(),
            disk: DiskImageCache::in_directory(PathBuf::new(), 10),
            dimensions,
        };
        let resource = Resource::Uri("https://example.test/image.png".into());
        let key = hash(&resource);
        cache.entries.insert(
            key,
            CacheEntry::Failed(ImageCacheError::Asset("failed".into())),
        );
        assert!(cache.retry_failed(&resource));
        assert!(!cache.entries.contains_key(&key));
        assert!(!cache.retry_failed(&resource));
    }
}
