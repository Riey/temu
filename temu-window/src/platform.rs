#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub type NativeWindow = self::windows::Window;
