//! The macOS side: a global hotkey through Carbon's RegisterEventHotKey
//! (no Accessibility permission needed), the Screen Recording permission,
//! macOS's own interactive capture (`screencapture -i`), the clipboard,
//! and small AppKit calls.

use objc2_app_kit::{NSApplicationActivationOptions, NSPasteboard, NSRunningApplication, NSSound};
use objc2_foundation::{NSData, NSString};
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;

// ---- Screen Recording ----

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Whether rsnap may record the screen (macOS asks once).
pub fn screen_access() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Ask for Screen Recording access (the system prompt, once) and open the
/// settings pane where it's switched on.
pub fn ask_for_screen_access() {
    unsafe { CGRequestScreenCaptureAccess() };
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
}

// ---- The capture shortcut (⌥⇧S) ----

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

type EventHandler = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: EventHandler,
        num_types: u32,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(key_code: u32, modifiers: u32, id: EventHotKeyID, target: *mut c_void, options: u32, out_ref: *mut *mut c_void) -> i32;
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
}

const KEY_S: u32 = 1;
const SHIFT: u32 = 1 << 9;
const OPTION: u32 = 1 << 11;

static HOTKEY: OnceLock<(Sender<()>, eframe::egui::Context)> = OnceLock::new();

extern "C" fn on_hotkey(_call: *mut c_void, _event: *mut c_void, _data: *mut c_void) -> i32 {
    if let Some((tx, ctx)) = HOTKEY.get() {
        let _ = tx.send(());
        ctx.request_repaint();
    }
    0
}

/// Register ⌥⇧S. Main thread only, once the app is running. Returns false
/// when another app already holds the shortcut.
pub fn register_hotkey(tx: Sender<()>, ctx: eframe::egui::Context) -> bool {
    if HOTKEY.set((tx, ctx)).is_err() {
        return true;
    }
    unsafe {
        let target = GetApplicationEventTarget();
        let spec = EventTypeSpec { event_class: fourcc(b"keyb"), event_kind: 5 /* kEventHotKeyPressed */ };
        let mut handler = std::ptr::null_mut();
        if InstallEventHandler(target, on_hotkey, 1, &spec, std::ptr::null_mut(), &mut handler) != 0 {
            return false;
        }
        let mut hotkey = std::ptr::null_mut();
        RegisterEventHotKey(KEY_S, OPTION | SHIFT, EventHotKeyID { signature: fourcc(b"rsnp"), id: 1 }, target, 0, &mut hotkey) == 0
    }
}

// ---- Capturing ----

/// Run macOS's interactive capture: drag a region, or press Space and click
/// a window (saved without its shadow; rsnap draws its own). Blocks until
/// done. `None` when cancelled with Esc.
pub fn capture_interactive() -> Option<image::RgbaImage> {
    let path: PathBuf = std::env::temp_dir().join(format!("rsnap-{}-{}.png", std::process::id(), rsnap_core_now()));
    let status = std::process::Command::new("/usr/sbin/screencapture").args(["-i", "-x", "-o"]).arg(&path).status().ok()?;
    if !status.success() || !path.exists() {
        return None;
    }
    let img = image::open(&path).ok().map(|i| i.to_rgba8());
    let _ = std::fs::remove_file(&path);
    img
}

fn rsnap_core_now() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

// ---- Clipboard and app ----

/// Put a PNG on the clipboard.
pub fn copy_png(png: &[u8]) {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    pb.setData_forType(Some(&NSData::with_bytes(png)), &NSString::from_str("public.png"));
}

pub fn play(sound: &str) {
    if let Some(s) = NSSound::soundNamed(&NSString::from_str(sound)) {
        s.play();
    }
}

pub fn activate() {
    NSRunningApplication::currentApplication().activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
}

pub fn hide() {
    NSRunningApplication::currentApplication().hide();
}

pub fn open_url(url: &str) {
    if url.starts_with("https://") {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

/// Local UTC offset in seconds.
pub fn utc_offset() -> i64 {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm.tm_gmtoff
    }
}
