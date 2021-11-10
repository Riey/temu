use crate::{TemuEvent, TemuPtyEvent, TemuWindow};

use crossbeam_channel::Sender;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, PSTR, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::ValidateRect;
use windows::Win32::System::{
    Com::{CoInitializeEx, COINIT_MULTITHREADED},
    LibraryLoader::GetModuleHandleA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DestroyWindow, DispatchMessageA, GetClientRect, GetMessageA,
    GetWindowLongPtrA, LoadCursorW, PostQuitMessage, RegisterClassA, SetWindowLongPtrA,
    TranslateMessage, CREATESTRUCTA, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA,
    IDC_ARROW, MSG, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_NCCREATE, WM_PAINT, WM_SIZE,
    WNDCLASSA, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use raw_window_handle::{windows::WindowsHandle, HasRawWindowHandle, RawWindowHandle};
pub struct Window {
    handle: WindowsHandle,
}

unsafe impl HasRawWindowHandle for Window {
    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Windows(self.handle)
    }
}

struct WindowContext {
    event_tx: Sender<TemuEvent>,
    #[allow(dead_code)]
    pty_event_tx: Sender<TemuPtyEvent>,
}

impl TemuWindow for Window {
    fn init(event_tx: Sender<crate::event::TemuEvent>, pty_event_tx: Sender<TemuPtyEvent>) -> Self {
        let ctx = WindowContext {
            event_tx,
            pty_event_tx,
        };
        let lparam = Box::leak(Box::new(ctx)) as *mut WindowContext;

        let mut handle = WindowsHandle::empty();
        unsafe {
            CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED).unwrap();
            let instance = GetModuleHandleA(None);
            debug_assert!(instance.0 != 0);

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_ARROW),
                hInstance: instance,
                lpszClassName: PSTR(b"temu\0".as_ptr() as _),
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);

            let hwnd = CreateWindowExA(
                Default::default(),
                "temu",
                "Temu",
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                800,
                500,
                None,
                None,
                instance,
                lparam.cast(),
            );

            handle.hwnd = hwnd.0 as _;
            handle.hinstance = instance.0 as _;
        }

        Self { handle }
    }

    fn run(self) {
        let mut message = MSG::default();

        unsafe {
            while GetMessageA(&mut message, HWND(0), 0, 0).into() {
                TranslateMessage(&message);
                DispatchMessageA(&mut message);
                if CLOSED.load(Ordering::Acquire) {
                    return;
                }
            }
        }
    }
}

static CLOSED: AtomicBool = AtomicBool::new(false);

extern "system" fn wndproc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ctx: &'static mut WindowContext;

        if matches!(message, WM_GETMINMAXINFO) {
            return LRESULT(0);
        }

        if message == WM_NCCREATE {
            let st = lparam.0 as *mut CREATESTRUCTA;
            ctx = (*st)
                .lpCreateParams
                .cast::<WindowContext>()
                .as_mut()
                .unwrap();
            SetWindowLongPtrA(hwnd, GWLP_USERDATA, ctx as *mut WindowContext as isize);
        } else {
            ctx = (GetWindowLongPtrA(hwnd, GWLP_USERDATA) as *mut WindowContext)
                .as_mut()
                .unwrap();
        }

        match message {
            WM_PAINT => {
                // ValidateRect(hwnd, std::ptr::null());
                ctx.event_tx.send(TemuEvent::Redraw).ok();
            }
            WM_SIZE => {
                let size = get_window_size(hwnd);
                ctx.event_tx
                    .send(TemuEvent::Resize {
                        width: size.cx as _,
                        height: size.cy as _,
                    })
                    .ok();
            }
            WM_DESTROY => {
                log::info!("WM_DESTROY");
                PostQuitMessage(0);
            }
            WM_CLOSE => {
                log::info!("WM_CLOSE");
                CLOSED.store(true, Ordering::Release);
                ctx.event_tx.send(TemuEvent::Close).ok();
                DestroyWindow(hwnd);
            }
            _ => return DefWindowProcA(hwnd, message, wparam, lparam),
        }
        LRESULT(0)
    }
}

unsafe fn get_window_size(hwnd: HWND) -> SIZE {
    let mut client_rect = RECT::default();
    GetClientRect(hwnd, &mut client_rect);
    SIZE {
        cx: client_rect.right - client_rect.left,
        cy: client_rect.bottom - client_rect.top,
    }
}
