#!/usr/bin/env ruby
# frozen_string_literal: true

require "cgi"
require "pathname"
require "set"
require "tmpdir"
require "uri"

SITE_PREFIX = "/tachyon/"

def decoded_reference(raw_reference)
  URI::DEFAULT_PARSER.unescape(CGI.unescapeHTML(raw_reference))
rescue URI::Error => error
  raise ArgumentError, "invalid URL encoding in #{raw_reference.inspect}: #{error.message}"
end

def html_identifiers(path)
  path.read.scan(/\bid=["']([^"']+)["']/).flatten.to_set
end

def check_site(site)
  root = Pathname(site).realpath
  failures = []

  html_paths = Dir.glob(root.join("**", "*.html").to_s).sort.map { |path| Pathname(path) }
  failures << "no generated HTML files found" if html_paths.empty?

  html_paths.each do |html|
    contents = html.read
    identifiers = html_identifiers(html)

    contents.scan(/<(?:a|img|link|script)\b[^>]+\b(?:href|src)=["']([^"']+)["']/i).flatten.each do |raw_reference|
      begin
        reference = decoded_reference(raw_reference)
      rescue ArgumentError => error
        failures << "#{html.relative_path_from(root)}: #{error.message}"
        next
      end
      next if reference.empty? || reference.start_with?("//")
      next if reference.match?(/\A[a-z][a-z0-9+.-]*:/i)
      next if reference == "#"

      path, anchor = reference.split("#", 2)
      path = path.to_s.split("?", 2).first.to_s
      if path.empty?
        if anchor && !identifiers.include?(anchor)
          failures << "#{html.relative_path_from(root)}: missing anchor ##{anchor}"
        end
        next
      end

      target = if path.start_with?(SITE_PREFIX)
        root.join(path.delete_prefix(SITE_PREFIX)).cleanpath
      elsif path.start_with?("/")
        failures << "#{html.relative_path_from(root)}: path is outside the #{SITE_PREFIX} Pages site: #{reference}"
        next
      else
        html.dirname.join(path).cleanpath
      end
      target = target.expand_path
      unless target.to_s == root.to_s || target.to_s.start_with?("#{root}/")
        failures << "#{html.relative_path_from(root)}: local target escapes the generated site: #{reference}"
        next
      end
      unless target.file? || target.directory?
        failures << "#{html.relative_path_from(root)}: missing local target #{reference}"
        next
      end

      next unless anchor && target.extname == ".html"

      unless html_identifiers(target).include?(anchor)
        failures << "#{html.relative_path_from(root)}: missing anchor ##{anchor} in #{path}"
      end
    end
  end

  site_404 = root.join("404.html")
  if !site_404.file?
    failures << "missing 404.html"
  elsif !site_404.read.include?(%(<base href="#{SITE_PREFIX}">))
    failures << "Pages site URL in 404.html is not #{SITE_PREFIX}"
  end

  failures
end

def write_fixture(root, index:, page: '<h1 id="target">Target</h1>', base: SITE_PREFIX)
  File.write(root.join("index.html"), index)
  File.write(root.join("page.html"), page)
  File.write(root.join("404.html"), %(<base href="#{base}">))
end

def self_test!
  Dir.mktmpdir("tachyon-doc-check-") do |directory|
    root = Pathname(directory)

    write_fixture(root, index: '<a href="page.html#target">valid</a>')
    abort "checker self-test failed: valid fixture was rejected" unless check_site(root).empty?

    write_fixture(root, index: '<a href="missing.html">broken file</a>')
    failures = check_site(root)
    abort "checker self-test failed: broken file passed" unless failures.any? { |failure| failure.include?("missing local target") }

    write_fixture(root, index: '<a href="page.html#missing">broken anchor</a>')
    failures = check_site(root)
    abort "checker self-test failed: broken anchor passed" unless failures.any? { |failure| failure.include?("missing anchor #missing") }

    write_fixture(root, index: '<a href="../../outside.html">escape</a>')
    failures = check_site(root)
    abort "checker self-test failed: escaping link passed" unless failures.any? { |failure| failure.include?("escapes the generated site") }

    write_fixture(root, index: '<a href="page.html">bad base</a>', base: "/")
    failures = check_site(root)
    abort "checker self-test failed: incorrect Pages base passed" unless failures.any? { |failure| failure.include?("is not #{SITE_PREFIX}") }
  end

  puts "Documentation checker self-test passed"
end

if ARGV == ["--self-test"]
  self_test!
  exit
end

site = ARGV.fetch(0) { abort "usage: #{$PROGRAM_NAME} SITE_DIRECTORY\n       #{$PROGRAM_NAME} --self-test" }
abort "usage: #{$PROGRAM_NAME} SITE_DIRECTORY" unless ARGV.length == 1

failures = check_site(site)
abort "Documentation link check failed:\n#{failures.join("\n")}" unless failures.empty?

puts "Documentation link check passed"
