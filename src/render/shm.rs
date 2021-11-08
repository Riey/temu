struct Shm {
    shm: RawFd,
    shm_file: ManuallyDrop<File>,
    shm_mem: *mut u8,
    shm_len: usize,
    name: CString,
}

unsafe impl Send for Shm {}
unsafe impl Sync for Shm {}

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe {
            munmap(self.shm_mem.cast(), self.shm_len);
            shm_unlink(self.name.as_ptr());
        }
    }
}

impl Shm {
    pub fn new(name: CString) -> Self {
        let mut shm = unsafe { shm_open(name.as_ptr(), O_CREAT | O_RDWR, 0o600) };
        let shm_len = 0;
        let shm_mem = unsafe {
            mmap(
                std::ptr::null_mut(),
                0,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                shm,
                0,
            )
            .cast::<u8>()
        };

        assert!(!shm_mem.is_null());

        Self {
            name,
            shm_file: ManuallyDrop::new(unsafe { File::from_raw_fd(shm) }),
            shm,
            shm_len,
            shm_mem,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: shm slice is live as long as file is available
        unsafe { std::slice::from_raw_parts(self.shm_mem, self.shm_len) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: shm slice is live as long as file is available
        unsafe { std::slice::from_raw_parts_mut(self.shm_mem, self.shm_len) }
    }

    pub fn resize(&mut self, new_size: usize) {
        unsafe {
            ftruncate(self.shm, new_size as i64).unwrap();
            self.shm_mem =
                mremap(self.shm_mem.cast(), self.shm_len, new_size, MREMAP_MAYMOVE).cast();
        }
        self.shm_len = new_size;
    }

    // this function must mutable method because &File can cause mutable aliasing shm_mem
    pub fn shm_file(&mut self) -> &File {
        &self.shm_file
    }
}

impl std::ops::Deref for Shm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl std::ops::DerefMut for Shm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_bytes_mut()
    }
}
