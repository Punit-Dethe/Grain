//! Windows lifetime ownership for extension companion processes.
// ── Windows Job Object (raw FFI — no extra `windows` crate features) ────────

/// Opaque handle wrapper for the Job Object (same layout as HANDLE).
#[cfg(windows)]
pub(crate) struct JobHandle(isize);

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            job_ffi::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
mod job_ffi {
    //! Minimal raw FFI for Job Objects. We declare only the 3 functions and
    //! 2 structs we actually need so we don't pull in `Win32_Security` /
    //! `Win32_System_JobObjects` from the `windows` crate (which bloats
    //! the binary by several MB and increases resident memory).
    use std::ffi::c_void;

    pub type HANDLE = isize;
    pub type BOOL = i32;

    // --- Job Object info class & limits ---
    pub const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x00002000;

    #[repr(C)]
    #[derive(Default)]
    pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        pub per_process_user_time_limit: i64,
        pub per_job_user_time_limit: i64,
        pub limit_flags: u32,
        pub minimum_working_set_size: usize,
        pub maximum_working_set_size: usize,
        pub active_process_limit: u32,
        pub affinity: usize,
        pub priority_class: u32,
        pub scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct IO_COUNTERS {
        pub read_operation_count: u64,
        pub write_operation_count: u64,
        pub other_operation_count: u64,
        pub read_transfer_count: u64,
        pub write_transfer_count: u64,
        pub other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        pub basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        pub io_info: IO_COUNTERS,
        pub process_memory_limit: usize,
        pub job_memory_limit: usize,
        pub peak_process_memory_used: usize,
        pub peak_job_memory_used: usize,
    }

    extern "system" {
        pub fn CreateJobObjectW(
            lp_job_attributes: *const c_void, // SECURITY_ATTRIBUTES* — we pass null
            lp_name: *const u16,              // optional name — we pass null
        ) -> HANDLE;

        pub fn SetInformationJobObject(
            h_job: HANDLE,
            job_object_information_class: u32,
            lp_job_object_information: *const c_void,
            cb_job_object_information_length: u32,
        ) -> BOOL;

        pub fn AssignProcessToJobObject(h_job: HANDLE, h_process: HANDLE) -> BOOL;

        pub fn CloseHandle(h_object: HANDLE) -> BOOL;

        pub fn GetLastError() -> u32;
    }
}

/// Create an anonymous Job Object configured with `KILL_ON_JOB_CLOSE`.
///
/// When the last handle to this Job Object is closed (which happens
/// automatically when the owning process exits), the OS kernel terminates
/// every process still assigned to the job. This is the production-grade
/// mechanism used by Chrome, VS Code, etc.
#[cfg(windows)]
pub(crate) fn create_job_object() -> Option<JobHandle> {
    use std::ptr;

    unsafe {
        let job = job_ffi::CreateJobObjectW(ptr::null(), ptr::null());
        if job == 0 {
            log::error!(
                "[GRAIN] failed to create Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
            return None;
        }

        let mut info = job_ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.basic_limit_information.limit_flags = job_ffi::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = job_ffi::SetInformationJobObject(
            job,
            job_ffi::JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<job_ffi::JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if ok == 0 {
            log::error!(
                "[GRAIN] failed to configure Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
            job_ffi::CloseHandle(job);
            return None;
        }

        log::info!("[GRAIN] child Job Object created (KILL_ON_JOB_CLOSE)");
        Some(JobHandle(job))
    }
}

/// Assign a child process to the Job Object so it is automatically killed
/// when the main process exits.
#[cfg(windows)]
pub(crate) fn assign_child_to_job(job: &JobHandle, child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;

    unsafe {
        let child_handle = child.as_raw_handle() as job_ffi::HANDLE;
        let ok = job_ffi::AssignProcessToJobObject(job.0, child_handle);
        if ok == 0 {
            log::warn!(
                "[GRAIN] failed to assign child to Job Object (win32 error {})",
                job_ffi::GetLastError()
            );
        } else {
            log::info!("[GRAIN] child assigned to Job Object (pid {})", child.id());
        }
    }
}
