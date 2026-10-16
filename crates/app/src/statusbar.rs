//! The menu bar icon: Capture, Open the last shot, Quit. Clicks arrive on
//! the main thread and are forwarded to the app through a channel.

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::{NSData, NSSize, NSString};
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    Capture,
    Open,
    Quit,
}

const TAG_CAPTURE: isize = 1;
const TAG_OPEN: isize = 2;
const TAG_QUIT: isize = 3;

pub struct TargetIvars {
    tx: Sender<MenuEvent>,
    ctx: egui::Context,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RsnapMenuTarget"]
    #[ivars = TargetIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(itemClicked:))]
        fn item_clicked(&self, sender: &NSMenuItem) {
            let event = match sender.tag() {
                TAG_CAPTURE => MenuEvent::Capture,
                TAG_OPEN => MenuEvent::Open,
                TAG_QUIT => MenuEvent::Quit,
                _ => return,
            };
            let _ = self.ivars().tx.send(event);
            self.ivars().ctx.request_repaint();
        }
    }
);

pub struct StatusBar {
    _item: Retained<NSStatusItem>,
    _menu: Retained<NSMenu>,
    _target: Retained<MenuTarget>,
}

impl StatusBar {
    /// Main thread only (the app's creation runs there).
    pub fn new(ctx: &egui::Context) -> Option<(StatusBar, Receiver<MenuEvent>)> {
        let mtm = MainThreadMarker::new()?;
        let (tx, rx) = channel();
        let target = MenuTarget::alloc(mtm).set_ivars(TargetIvars { tx, ctx: ctx.clone() });
        let target: Retained<MenuTarget> = unsafe { msg_send![super(target), init] };
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let data = NSData::with_bytes(include_bytes!("../../../assets/menubar.png"));
            if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
                image.setSize(NSSize::new(18.0, 18.0));
                image.setTemplate(true);
                button.setImage(Some(&image));
            }
            button.setToolTip(Some(&NSString::from_str("rsnap")));
        }
        let menu = NSMenu::new(mtm);
        let add = |title: &str, action: Option<Sel>, tag: isize, key: &str| {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), action, &NSString::from_str(key))
            };
            item.setTag(tag);
            let t: &AnyObject = &target;
            unsafe { item.setTarget(Some(t)) };
            menu.addItem(&item);
        };
        add("Capture  ⌥⇧S", Some(sel!(itemClicked:)), TAG_CAPTURE, "");
        add("Open last shot", Some(sel!(itemClicked:)), TAG_OPEN, "");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add("Quit rsnap", Some(sel!(itemClicked:)), TAG_QUIT, "");
        item.setMenu(Some(&menu));
        Some((StatusBar { _item: item, _menu: menu, _target: target }, rx))
    }
}
