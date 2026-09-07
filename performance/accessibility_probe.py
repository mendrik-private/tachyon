"""Private-session AT-SPI integration checks for synthetic Mineral documents.

The probe never connects to the physical desktop's accessibility bus. The
capture harness owns a separate D-Bus process group and passes its environment
to both the app and this /usr/bin/python3 GI client.
"""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from xml.sax.saxutils import escape


class SnapshotChanged(RuntimeError):
    """A child disappeared between AT-SPI child-count and child lookup."""


def stable_traversal(visit, root, nodes, objects):
    # Native trees change during disclosure reflow/virtualized scrolling.
    # Retry the whole traversal, never publish a partial/mixed snapshot or
    # silently drop the missing child. Other probe errors remain failures.
    for attempt in range(4):
        nodes.clear()
        objects.clear()
        try:
            visit(root, None, 0)
            return
        except Exception as error:
            disappeared = (getattr(error, 'domain', None) == 'atspi_error'
                           and getattr(error, 'code', None) == 1
                           and getattr(error, 'message', '').startswith(
                               "Unknown object '/org/a11y/atspi/accessible/"))
            if attempt == 3 or not (isinstance(error, SnapshotChanged) or disappeared):
                raise
            time.sleep(0.05)


@contextmanager
def private_bus(environment, enabled=True):
    # Whitelist only accessibility activation. A normal session bus can launch
    # unrelated desktop services (including GVFS/FUSE) inside a test runtime.
    with tempfile.TemporaryDirectory(prefix="mineral-atspi-bus-") as directory:
        services = Path(directory) / "services"
        services.mkdir()
        shutil.copyfile("/usr/share/dbus-1/services/org.a11y.Bus.service",
                        services / "org.a11y.Bus.service")
        config = Path(directory) / "bus.conf"
        config.write_text(
            '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>'
            '<auth>EXTERNAL</auth><servicedir>' + escape(str(services)) + '</servicedir>'
            '<policy context="default"><allow send_destination="*" eavesdrop="true"/>'
            '<allow eavesdrop="true"/><allow own="*"/></policy></busconfig>')
        with _private_bus(environment, config, enabled) as env:
            yield env


@contextmanager
def _private_bus(environment, config, enabled):
    env = dict(environment, GSETTINGS_BACKEND="memory", GIO_USE_VFS="local")
    for key in ("AT_SPI_BUS", "AT_SPI_BUS_ADDRESS", "ATSPI_BUS_ADDRESS"):
        env.pop(key, None)
    bus = subprocess.Popen(
        ["dbus-daemon", f"--config-file={config}", "--nofork", "--print-address=1"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
        env=env, start_new_session=True,
    )
    try:
        if not select.select([bus.stdout], [], [], 10)[0]:
            raise RuntimeError("Private accessibility session bus did not start")
        address = bus.stdout.readline().strip()
        if not address.startswith("unix:") or bus.poll() is not None:
            raise RuntimeError("Invalid private accessibility session bus")
        env["DBUS_SESSION_BUS_ADDRESS"] = address
        env["MINERAL_PRIVATE_ATSPI_BUS"] = address
        for name in ("IsEnabled", "ScreenReaderEnabled"):
            subprocess.run([
                "gdbus", "call", "--session", "--dest", "org.a11y.Bus",
                "--object-path", "/org/a11y/bus", "--method",
                "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", name,
                "<true>" if enabled else "<false>",
            ], env=env, check=True, capture_output=True, text=True, timeout=10)
        yield env
    finally:
        # Only the process group created above: never the physical session bus.
        try:
            os.killpg(bus.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            bus.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(bus.pid, signal.SIGKILL)
            bus.wait(timeout=5)
        bus.stdout.close()


def snapshot(pid, exercise=False, toggle_html=False, exercise_gallery=False):
    private_address = os.environ.get("MINERAL_PRIVATE_ATSPI_BUS")
    if not private_address or os.environ.get("DBUS_SESSION_BUS_ADDRESS") != private_address:
        raise RuntimeError("Accessibility probes require the harness-owned private session bus")
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi

    Atspi.set_timeout(2000, 10000)
    desktop = Atspi.get_desktop(0)
    deadline = time.monotonic() + 10
    target = None
    while target is None:
        desktop.clear_cache()
        for index in range(desktop.get_child_count()):
            child = desktop.get_child_at_index(index)
            if child.get_process_id() == pid and child.get_child_count():
                target = child
                break
        if time.monotonic() > deadline:
            raise RuntimeError("Mineral did not publish an AT-SPI application tree")
        if target is None:
            time.sleep(0.05)

    nodes = []
    objects = {}

    def extents(node):
        rect = node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
        return dict(x=rect.x, y=rect.y, width=rect.width, height=rect.height)

    def visit(node, parent, depth):
        if node is None:
            raise SnapshotChanged("AT-SPI child disappeared during traversal")
        if depth > 64 or len(nodes) >= 10000:
            raise RuntimeError("Accessibility traversal exceeded synthetic fixture bounds")
        index = len(nodes)
        node.clear_cache()
        actions = node.get_action_iface()
        record = {
            "parent": parent, "role": node.get_role_name(), "name": node.get_name(),
            "path": node.path,
            "interfaces": list(node.get_interfaces()),
            "attributes": dict(node.get_attributes() or {}),
            "states": [state.value_nick for state in node.get_state_set().get_states()],
            "actions": [actions.get_action_name(i) for i in range(actions.get_n_actions())] if actions else [],
        }
        if node.get_component_iface():
            record["bounds"] = extents(node)
        objects[node.path] = node
        nodes.append(record)
        for i in range(node.get_child_count()):
            visit(node.get_child_at_index(i), index, depth + 1)

    stable_traversal(visit, target, nodes, objects)
    result = {"nodes": nodes}
    if exercise_gallery:
        images = [node for node in nodes if node["role"] == "image"]
        expected = ["Linked landscape description", "Unlinked landscape description"]
        if [node["name"] for node in images] != expected:
            raise RuntimeError("Gallery image descriptions/order are missing or duplicated")
        linked, unlinked = images
        parent = nodes[linked["parent"]]
        if parent["role"] != "link" or "click" not in parent["actions"]:
            raise RuntimeError("Linked gallery image lacks its actionable native link parent")
        hyperlink = objects[parent["path"]].get_hyperlink()
        if not hyperlink or hyperlink.get_uri(0) != "#gallery-destination":
            raise RuntimeError("Native gallery link lost its authored target")
        if nodes[unlinked["parent"]]["role"] == "link" or "click" in unlinked["actions"]:
            raise RuntimeError("Unlinked image must not become a link")
        heading = next(node for node in nodes if node["role"] == "heading"
                       and node["name"] == "Gallery destination")
        viewport = next(node["bounds"] for node in nodes
                        if node["name"] == "Markdown document editor")
        if heading["bounds"]["y"] < viewport["y"] + viewport["height"]:
            raise RuntimeError("Gallery destination must initially be offscreen")
        action = objects[parent["path"]].get_action_iface()
        click = parent["actions"].index("click")
        if not action.do_action(click):
            raise RuntimeError("Native image-link activation was rejected")
        deadline = time.monotonic() + 5
        while True:
            after = extents(objects[heading["path"]])
            if viewport["y"] <= after["y"] < viewport["y"] + viewport["height"]:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError("Native image link did not reveal its authored destination")
            time.sleep(0.05)
        result["gallery"] = dict(image_descriptions=expected, link_path=parent["path"],
                                 link_target=hyperlink.get_uri(0),
                                 destination_before=heading["bounds"], destination_after=after,
                                 identities_retained=all(objects[node["path"]].get_name() == node["name"]
                                                         for node in images), passes=True)
    if toggle_html:
        summary = next(record for record in nodes
                       if record["role"] == "button" and record["name"] == "Closed HTML disclosure")
        action = objects[summary["path"]].get_action_iface()
        from gi.repository import GLib
        events = []

        def expansion_changed(event, _data=None):
            if event.source.path == summary["path"]:
                events.append(dict(type=event.type, source_path=event.source.path,
                                   expanded=bool(event.detail1)))

        listener = Atspi.EventListener.new(expansion_changed)
        event_type = "object:state-changed:expanded"
        if not listener.register(event_type):
            raise RuntimeError("Could not subscribe to native disclosure state changes")
        try:
            if not action or not action.get_n_actions() or not action.do_action(0):
                raise RuntimeError("HTML disclosure has no working native action")
            context = GLib.MainContext.default()
            deadline = time.monotonic() + 10
            while not events and time.monotonic() < deadline:
                for _ in range(100):
                    if not context.pending():
                        break
                    context.iteration(False)
                time.sleep(0.01)
            if not events:
                raise RuntimeError("Disclosure activation emitted no native expanded-state event")
        finally:
            listener.deregister(event_type)
        result["disclosure_action"] = summary["path"]
        result["disclosure_state_events"] = events
    if exercise:
        heading = next(record for record in nodes if record["role"] == "heading"
                       and record["name"] == "7. Final offscreen heading")
        viewport = next(record["bounds"] for record in nodes
                        if record["name"] == "Markdown document editor")
        if heading["bounds"]["y"] <= viewport["y"] + viewport["height"]:
            raise RuntimeError("Accessibility reveal fixture must begin offscreen")
        original_headings = [(record["path"], record["name"]) for record in nodes
                             if record["role"] == "heading"]
        leaf = objects[heading["path"]]
        if not leaf.get_component_iface().scroll_to(Atspi.ScrollType.ANYWHERE):
            raise RuntimeError("Offscreen heading has no native reveal action")
        deadline = time.monotonic() + 5
        while True:
            after = extents(leaf)
            if viewport["y"] <= after["y"] < viewport["y"] + viewport["height"]:
                break
            if time.monotonic() > deadline:
                raise RuntimeError("Native accessibility reveal did not scroll the original heading")
            time.sleep(0.05)
        result["reveal"] = dict(before=heading["bounds"], after=after, viewport=viewport)
        result["heading_ids_stable_after_scroll"] = all(
            objects[path].get_name() == name for path, name in original_headings)
        checkbox = next(record for record in nodes if record["role"] == "check box"
                        and "checked" not in record["states"])
        checkbox_object = objects[checkbox["path"]]
        action = checkbox_object.get_action_iface()
        if not action or not action.get_n_actions() or not action.do_action(0):
            raise RuntimeError("Task checkbox has no working native action")
        deadline = time.monotonic() + 5
        while True:
            checkbox_object.clear_cache()
            if checkbox_object.get_state_set().contains(Atspi.StateType.CHECKED):
                break
            if time.monotonic() > deadline:
                raise RuntimeError("Native checkbox action did not update checked state")
            time.sleep(0.05)
        result["task_action"] = dict(path=checkbox["path"], name=checkbox["name"], checked=True)
    return result


if __name__ == "__main__":
    print(json.dumps(snapshot(int(sys.argv[1]), exercise="--exercise" in sys.argv[2:],
                              toggle_html="--toggle-html" in sys.argv[2:],
                              exercise_gallery="--exercise-gallery" in sys.argv[2:])))
