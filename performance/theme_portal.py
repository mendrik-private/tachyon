"""Owned settings-portal fixture; never connects to the user's desktop bus."""
from contextlib import contextmanager
import os
from pathlib import Path
import select
import subprocess
import sys


def require_private_bus(environment):
    address = environment.get("DBUS_SESSION_BUS_ADDRESS")
    if not address or address != environment.get("TACHYON_PRIVATE_ATSPI_BUS"):
        raise RuntimeError("Theme fixture requires the harness-owned private session bus")


@contextmanager
def settings_portal(environment, appearance):
    require_private_bus(environment)
    process = subprocess.Popen(
        ["/usr/bin/python3", str(Path(__file__).resolve()), appearance],
        env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    try:
        if not select.select([process.stdout], [], [], 5)[0] or process.stdout.readline().strip() != "READY":
            raise RuntimeError("Private settings portal did not become ready")
        yield
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        process.stdout.close()
        process.stderr.close()


def set_appearance(environment, appearance):
    require_private_bus(environment)
    subprocess.run([
        "gdbus", "call", "--session", "--dest", "org.freedesktop.portal.Desktop",
        "--object-path", "/org/freedesktop/portal/desktop",
        "--method", "org.tachyon.ThemeHarness.SetAppearance", appearance,
    ], env=environment, check=True, capture_output=True, text=True, timeout=5)


def serve(appearance):
    require_private_bus(os.environ)
    from gi.repository import Gio, GLib
    schemes = {"dark": 1, "light": 2}
    current = schemes[appearance]
    interface = "org.freedesktop.portal.Settings"
    path = "/org/freedesktop/portal/desktop"
    xml = """<node>
      <interface name="org.freedesktop.portal.Settings">
        <method name="Read"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
        <property name="version" type="u" access="read"/>
        <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
      </interface>
      <interface name="org.tachyon.ThemeHarness">
        <method name="SetAppearance"><arg type="s" direction="in"/></method>
      </interface>
    </node>"""
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def method(bus, sender, object_path, iface, name, args, invocation):
        nonlocal current
        if iface == "org.tachyon.ThemeHarness":
            mode, = args.unpack()
            if mode not in schemes:
                invocation.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs", "Unknown appearance")
                return
            current = schemes[mode]
            bus.emit_signal(None, path, interface, "SettingChanged", GLib.Variant(
                "(ssv)", ("org.freedesktop.appearance", "color-scheme", GLib.Variant("u", current))))
            invocation.return_value(GLib.Variant("()", ()))
        elif args.unpack() == ("org.freedesktop.appearance", "color-scheme"):
            # Settings.Read uses the legacy nested variant; ashpd unwraps it.
            invocation.return_value(GLib.Variant("(v)", (GLib.Variant("v", GLib.Variant("u", current)),)))
        else:
            invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "Unknown setting")

    info = Gio.DBusNodeInfo.new_for_xml(xml)
    for iface in info.interfaces:
        connection.register_object(path, iface, method, lambda *_: GLib.Variant("u", 1), None)
    result = connection.call_sync(
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "RequestName", GLib.Variant("(su)", ("org.freedesktop.portal.Desktop", 4)),
        GLib.VariantType.new("(u)"), Gio.DBusCallFlags.NONE, 5000, None,
    )
    if result.unpack() != (1,):
        raise RuntimeError("Private settings portal name was already owned")
    print("READY", flush=True)
    GLib.MainLoop().run()


if __name__ == "__main__":
    serve(sys.argv[1])
