#[cfg(all(windows, feature = "windows"))]
pub mod windows;

#[cfg(all(windows, feature = "windows"))]
pub type NativeWindow = self::windows::Window;

#[cfg(feature = "winit")]
pub mod winit;

pub type NativeWindow = self::winit::WinitWindow;
