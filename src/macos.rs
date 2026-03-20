//! macOS-specific integration: native menu bar setup and notifications.
//!
//! All code in this module is compiled only on macOS.

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};

/// Creates and installs the macOS native menu bar.
///
/// This replaces winit's default menu with a fully custom menu that includes:
/// - App menu (About, Hide, Quit, …)
/// - Edit menu (Copy, Paste, Select All)
/// - View menu (Full Screen)
/// - Window menu (Minimize, Zoom)
///
/// Must be called from the main thread after `NSApplication::sharedApplication`
/// has been initialised (i.e. after `EventLoop::new()`).
pub fn setup_menu_bar() {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("setup_menu_bar called off main thread — skipping");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);

    let menu_bar = NSMenu::new(mtm);

    // ── App menu ───────────────────────────────────────────────────────────
    let app_menu_item = make_menu_item(mtm, "");
    // SAFETY: `mtm.alloc()` is the correct allocator for `MainThreadOnly` AppKit
    // types (they do not implement `IsAllocableAnyThread`).  `initWithTitle` is
    // a designated initialiser and is safe to call after `mtm.alloc()`.
    let app_menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &ns_string("")) };

    // About Minal
    let about = make_item_with_action(mtm, "About Minal", "orderFrontStandardAboutPanel:", "");
    app_menu.addItem(&about);

    // Separator
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    // Hide Minal (Cmd+H)
    let hide = make_item_with_action(mtm, "Hide Minal", "hide:", "h");
    hide.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    app_menu.addItem(&hide);

    // Hide Others (Cmd+Opt+H)
    let hide_others = make_item_with_action(mtm, "Hide Others", "hideOtherApplications:", "h");
    hide_others.setKeyEquivalentModifierMask(
        NSEventModifierFlags::NSEventModifierFlagCommand
            | NSEventModifierFlags::NSEventModifierFlagOption,
    );
    app_menu.addItem(&hide_others);

    // Show All
    let show_all = make_item_with_action(mtm, "Show All", "unhideAllApplications:", "");
    app_menu.addItem(&show_all);

    // Separator
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));

    // Quit (Cmd+Q)
    let quit = make_item_with_action(mtm, "Quit Minal", "terminate:", "q");
    quit.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    app_menu.addItem(&quit);

    app_menu_item.setSubmenu(Some(&app_menu));
    menu_bar.addItem(&app_menu_item);

    // ── Edit menu ──────────────────────────────────────────────────────────
    let edit_menu_item = make_menu_item(mtm, "Edit");
    // SAFETY: Same as `app_menu` above — `mtm.alloc()` for `MainThreadOnly` types.
    let edit_menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &ns_string("Edit")) };

    // Copy (Cmd+C)
    let copy = make_item_with_action(mtm, "Copy", "copy:", "c");
    copy.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    edit_menu.addItem(&copy);

    // Paste (Cmd+V)
    let paste = make_item_with_action(mtm, "Paste", "paste:", "v");
    paste.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    edit_menu.addItem(&paste);

    // Select All (Cmd+A)
    let select_all = make_item_with_action(mtm, "Select All", "selectAll:", "a");
    select_all.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    edit_menu.addItem(&select_all);

    edit_menu_item.setSubmenu(Some(&edit_menu));
    menu_bar.addItem(&edit_menu_item);

    // ── View menu ──────────────────────────────────────────────────────────
    let view_menu_item = make_menu_item(mtm, "View");
    // SAFETY: See above.
    let view_menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &ns_string("View")) };

    // Enter Full Screen (Cmd+Ctrl+F)
    let fullscreen = make_item_with_action(mtm, "Enter Full Screen", "toggleFullScreen:", "f");
    fullscreen.setKeyEquivalentModifierMask(
        NSEventModifierFlags::NSEventModifierFlagCommand
            | NSEventModifierFlags::NSEventModifierFlagControl,
    );
    view_menu.addItem(&fullscreen);

    view_menu_item.setSubmenu(Some(&view_menu));
    menu_bar.addItem(&view_menu_item);

    // ── Window menu ────────────────────────────────────────────────────────
    let window_menu_item = make_menu_item(mtm, "Window");
    // SAFETY: See above.
    let window_menu = unsafe { NSMenu::initWithTitle(mtm.alloc(), &ns_string("Window")) };

    // Minimize (Cmd+M)
    let minimize = make_item_with_action(mtm, "Minimize", "performMiniaturize:", "m");
    minimize.setKeyEquivalentModifierMask(NSEventModifierFlags::NSEventModifierFlagCommand);
    window_menu.addItem(&minimize);

    // Zoom
    let zoom = make_item_with_action(mtm, "Zoom", "performZoom:", "");
    window_menu.addItem(&zoom);

    window_menu_item.setSubmenu(Some(&window_menu));
    menu_bar.addItem(&window_menu_item);

    // Install the menu bar.  `setMainMenu` is a safe AppKit method (not marked
    // unsafe in the objc2 bindings) that may only be called on the main thread,
    // which is guaranteed by `mtm`.
    app.setMainMenu(Some(&menu_bar));

    tracing::info!("macOS menu bar installed");
}

/// Sends a macOS user notification via `osascript`.
///
/// This is a simple approach that shells out to AppleScript so that no
/// additional entitlements or Info.plist keys are required during development.
#[allow(dead_code)] // Public API reserved for future notification integration.
pub fn send_notification(title: &str, body: &str) {
    let script = format!(
        "display notification {body_q} with title {title_q}",
        body_q = applescript_quote(body),
        title_q = applescript_quote(title),
    );
    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(out) if out.status.success() => {
            tracing::debug!("Notification sent: {title}");
        }
        Ok(out) => {
            tracing::warn!(
                "osascript returned non-zero: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            tracing::warn!("Failed to send notification via osascript: {e}");
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Wraps `s` in an AppleScript double-quoted string, escaping backslash and `"`.
fn applescript_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_applescript_quote() {
        assert_eq!(applescript_quote("hello"), "\"hello\"");
        assert_eq!(applescript_quote("he\"llo"), "\"he\\\"llo\"");
        assert_eq!(applescript_quote("he\\llo"), "\"he\\\\llo\"");
        assert_eq!(applescript_quote(""), "\"\"");
    }
}

/// Creates an `NSString` from a Rust `&str`.
///
/// `NSString::from_str` performs a safe UTF-8 copy of the Rust string slice.
fn ns_string(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// Creates an empty `NSMenuItem` with the given title (used as submenu containers).
///
/// # Safety
/// `NSMenuItem::initWithTitle_action_keyEquivalent` is a standard AppKit
/// initialiser.  `mtm.alloc()` is the correct allocator for `MainThreadOnly`
/// types (they do not implement `IsAllocableAnyThread`).
fn make_menu_item(mtm: MainThreadMarker, title: &str) -> Retained<NSMenuItem> {
    // SAFETY: `mtm.alloc::<NSMenuItem>()` allocates an uninitialized `NSMenuItem`
    // on the main thread; the subsequent `initWithTitle_action_keyEquivalent`
    // fully initialises it.  `None` is a valid action for a submenu container.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &ns_string(title),
            None,
            &ns_string(""),
        )
    }
}

/// Creates an `NSMenuItem` with the given title, AppKit selector, and key equivalent.
///
/// The selector is registered at runtime using `Sel::register` because the
/// `sel!` macro requires compile-time literals.
fn make_item_with_action(
    mtm: MainThreadMarker,
    title: &str,
    action_selector: &str,
    key: &str,
) -> Retained<NSMenuItem> {
    // `Sel::register` is a safe function that registers or looks up the selector
    // name in the Objective-C runtime.
    let sel = Sel::register(action_selector);
    // SAFETY: `mtm.alloc()` allocates the `NSMenuItem` on the main thread.
    // `initWithTitle_action_keyEquivalent` is the standard AppKit initialiser.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &ns_string(title),
            Some(sel),
            &ns_string(key),
        )
    }
}
