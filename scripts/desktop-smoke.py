#!/usr/bin/env python3
"""Verify native desktop startup and OS window-close on Linux or Windows."""
import ctypes
import http.cookiejar
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

def close_native_window(display_name):
    """Send the same WM_DELETE_WINDOW request a window manager sends on X11."""
    x11 = ctypes.CDLL("libX11.so.6")
    pointer, window_type = ctypes.c_void_p, ctypes.c_ulong
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = pointer
    x11.XDefaultRootWindow.argtypes = [pointer]
    x11.XDefaultRootWindow.restype = window_type
    x11.XQueryTree.argtypes = [pointer, window_type, ctypes.POINTER(window_type), ctypes.POINTER(window_type), ctypes.POINTER(ctypes.POINTER(window_type)), ctypes.POINTER(ctypes.c_uint)]
    x11.XFetchName.argtypes = [pointer, window_type, ctypes.POINTER(ctypes.c_char_p)]
    x11.XInternAtom.argtypes = [pointer, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = window_type
    x11.XFree.argtypes = [pointer]
    x11.XCloseDisplay.argtypes = [pointer]
    class Message(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int), ("display", pointer), ("window", window_type), ("message_type", window_type), ("format", ctypes.c_int), ("data", ctypes.c_long * 5)]
    class Event(ctypes.Union):
        _fields_ = [("message", Message), ("padding", ctypes.c_long * 24)]
    x11.XSendEvent.argtypes = [pointer, window_type, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
    connection = x11.XOpenDisplay(display_name.encode())
    assert connection
    root_window, parent = window_type(), window_type()
    children, count = ctypes.POINTER(window_type)(), ctypes.c_uint()
    try:
        assert x11.XQueryTree(connection, x11.XDefaultRootWindow(connection), ctypes.byref(root_window), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
        target = None
        for index in range(count.value):
            name = ctypes.c_char_p()
            if x11.XFetchName(connection, children[index], ctypes.byref(name)) and name.value:
                if name.value.decode() == "OBS WoW Companion":
                    target = children[index]
                x11.XFree(name)
        x11.XFree(children)
        assert target, "Native companion window missing"
        event = Event()
        event.message.type = 33  # ClientMessage
        event.message.display = connection
        event.message.window = target
        event.message.message_type = x11.XInternAtom(connection, b"WM_PROTOCOLS", 0)
        event.message.format = 32
        event.message.data[0] = x11.XInternAtom(connection, b"WM_DELETE_WINDOW", 0)
        assert x11.XSendEvent(connection, target, 0, 0, ctypes.byref(event))
    finally:
        x11.XCloseDisplay(connection)

def close_windows_window(pid):
    """Send WM_CLOSE only to the window belonging to this test process."""
    from ctypes import wintypes
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user32.EnumWindows.restype = wintypes.BOOL
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
    user32.PostMessageW.restype = wintypes.BOOL
    matches = []
    @callback_type
    def visit(window, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(owner))
        if owner.value == pid:
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(window, title, len(title))
            if title.value == "OBS WoW Companion":
                matches.append(window)
        return True
    assert user32.EnumWindows(visit, 0)
    assert len(matches) == 1, "Expected one native companion window owned by the test process"
    assert user32.PostMessageW(matches[0], 0x0010, 0, 0), "WM_CLOSE failed"


root = Path(tempfile.mkdtemp(prefix="owc-desktop-"))
display = ":122"
if os.name != "nt" and Path("/tmp/.X122-lock").exists():
    raise SystemExit("Private test display is already in use")
env = os.environ.copy()
env.update(DISPLAY=display, GDK_BACKEND="x11", LIBGL_ALWAYS_SOFTWARE="1", XDG_CONFIG_HOME=str(root / "config"))
with (root / "xvfb.log").open("w") as xlog, (root / "app.log").open("w") as log:
    xvfb = None if os.name == "nt" else subprocess.Popen(["Xvfb", display, "-screen", "0", "1440x1000x24", "-nolisten", "tcp"], stdout=xlog, stderr=xlog)
    app = None
    try:
        time.sleep(1)
        if xvfb is not None and xvfb.poll() is not None:
            raise RuntimeError("Xvfb failed")
        app = subprocess.Popen([os.environ.get("OWC_BINARY", "target/debug/obs-wow-companion.exe" if os.name == "nt" else "target/debug/obs-wow-companion"), "--data-dir", str(root / "data")], env=env, stdout=log, stderr=log)
        url = None
        for _ in range(150):
            time.sleep(0.2)
            match = re.search(r"http://127\.0\.0\.1:\d+/session/[a-f0-9-]+", (root / "app.log").read_text())
            if match:
                url = match.group()
                break
            if app.poll() is not None:
                raise RuntimeError((root / "app.log").read_text())
        assert url, "Service URL missing"
        base = url.split("/session/")[0]
        try:
            urllib.request.urlopen(base + "/api/status")
            raise AssertionError("Unauthenticated API access accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 401
        client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
        client.open(url).read()
        state = json.load(client.open(base + "/api/status"))
        assert state["connected"] is False
        request = urllib.request.Request(base + "/api/action", data=b'{"action":"set_marker_visibility","visible":true}', headers={"Content-Type":"application/json", "X-OWC-Request":"1"})
        result = json.load(client.open(request))
        assert result["ok"] is True
        request.add_header("Origin", "http://untrusted.invalid")
        try:
            client.open(request)
            raise AssertionError("Cross-origin mutation accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 403
        # Linux uses a private XDG directory. Windows known-folder APIs do not
        # honour an APPDATA override, so do not alter the user's Startup folder.
        for enabled in (() if os.name == "nt" else (True, False)):
            request = urllib.request.Request(base + "/api/action", data=json.dumps({"action":"autostart", "enabled":enabled}).encode(), headers={"Content-Type":"application/json", "X-OWC-Request":"1"})
            client.open(request).read()
            assert (root / "config/autostart/obs-wow-companion.desktop").exists() == enabled
        time.sleep(4)
        if os.name != "nt":
            subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "x11grab", "-video_size", "1440x1000", "-i", display, "-frames:v", "1", str(root / "desktop.png")], check=True)
        assert app.poll() is None, (root / "app.log").read_text()
        assert "Gtk-WARNING" not in (root / "app.log").read_text(), (root / "app.log").read_text()
        if os.name == "nt":
            close_windows_window(app.pid)
        else:
            close_native_window(display)
        assert app.wait(timeout=10) == 0
        print(f"Desktop/API and native window-close quit passed; evidence: {root}")
    finally:
        if app is not None and app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
        if xvfb is not None:
            xvfb.terminate()
            xvfb.wait(timeout=5)
