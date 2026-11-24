//! Bounded reads of the foreground application's focused Accessibility element.

use std::ptr;

use core_foundation::base::{CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;

const AX_TIMEOUT_SECONDS: f32 = 0.050;
const MAX_TEXT_UNITS: isize = 4096;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> CFTypeRef;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: CFTypeRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> i32;
}

struct FocusedElement {
    pid: libc::pid_t,
    element: CFType,
}

// SAFETY: AXUIElement is an immutable Core Foundation proxy. The watch moves
// its retained proxy to one blocking task at a time and never shares it.
unsafe impl Send for FocusedElement {}

impl FocusedElement {
    fn open() -> Option<Self> {
        if unsafe { AXIsProcessTrusted() } == 0 {
            return None;
        }
        let pid = foreground_pid()?;
        if pid <= 0 || pid == std::process::id() as libc::pid_t {
            return None;
        }
        // CreateApplication and CopyAttributeValue both transfer ownership.
        let application = unsafe { AXUIElementCreateApplication(pid) };
        if application.is_null() {
            return None;
        }
        let application = unsafe { CFType::wrap_under_create_rule(application) };
        if !set_timeout(&application) {
            return None;
        }
        let element = attribute(&application, "AXFocusedUIElement")?;
        if element.type_of() != unsafe { AXUIElementGetTypeID() } || !set_timeout(&element) {
            return None;
        }
        if string(&element, "AXSubrole", 64).as_deref() == Some("AXSecureTextField")
            || string(&element, "AXRole", 64).as_deref() == Some("AXSecureTextField")
        {
            return None;
        }
        Some(Self { pid, element })
    }

    fn editable_text(&self) -> Option<String> {
        let role = string(&self.element, "AXRole", 64)?;
        if !matches!(role.as_str(), "AXTextField" | "AXTextArea") {
            return None;
        }
        let mut settable = 0;
        let key = CFString::from_static_string("AXValue");
        let result = unsafe {
            AXUIElementIsAttributeSettable(
                self.element.as_CFTypeRef(),
                key.as_concrete_TypeRef(),
                &mut settable,
            )
        };
        (result == 0 && settable != 0).then_some(())?;
        self.read_string("AXValue")
    }

    fn read_string(&self, key: &'static str) -> Option<String> {
        let value = string(&self.element, key, MAX_TEXT_UNITS)?;
        (foreground_pid() == Some(self.pid)).then_some(value)
    }
}

pub(super) struct Field(Option<FocusedElement>);

impl Field {
    pub(super) fn open() -> Option<(Self, String)> {
        let field = FocusedElement::open()?;
        let text = field.editable_text()?;
        Some((Self(Some(field)), text))
    }

    pub(super) async fn read(&mut self) -> Option<String> {
        let original = self.0.take()?;
        let (original, value) = tauri::async_runtime::spawn_blocking(move || {
            let current = FocusedElement::open().filter(|current| {
                current.pid == original.pid && current.element == original.element
            });
            let value = current.and_then(|element| element.editable_text());
            (original, value)
        })
        .await
        .ok()?;
        self.0 = Some(original);
        value
    }
}

pub(crate) fn read_focused_selection() -> Option<String> {
    let selection = FocusedElement::open()?.read_string("AXSelectedText")?;
    let selection = selection.trim();
    (!selection.is_empty()).then(|| selection.to_owned())
}

fn foreground_pid() -> Option<libc::pid_t> {
    autoreleasepool(|_| {
        Some(
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()?
                .processIdentifier(),
        )
    })
}

fn set_timeout(element: &CFType) -> bool {
    unsafe { AXUIElementSetMessagingTimeout(element.as_CFTypeRef(), AX_TIMEOUT_SECONDS) == 0 }
}

fn attribute(element: &CFType, name: &'static str) -> Option<CFType> {
    let key = CFString::from_static_string(name);
    let mut value = ptr::null();
    let result = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef(),
            key.as_concrete_TypeRef(),
            &mut value,
        )
    };
    (result == 0 && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
}

fn string(element: &CFType, name: &'static str, limit: isize) -> Option<String> {
    let value = attribute(element, name)?.downcast_into::<CFString>()?;
    (value.char_len() <= limit).then(|| value.to_string())
}
