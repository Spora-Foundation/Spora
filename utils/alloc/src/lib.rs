#[cfg(not(feature = "heap"))]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
extern "C" {
    fn mi_option_set_enabled(_: MiOption, val: bool);
}

#[cfg(not(feature = "heap"))]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[allow(dead_code)]
#[repr(C)]
enum MiOption {
    // Mirrors mimalloc's `mi_option_purge_decommits` discriminator.
    PurgeDecommits = 5,
}

#[cfg(not(feature = "heap"))]
use mimalloc::MiMalloc;
#[cfg(not(feature = "heap"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub fn init_allocator_with_default_settings() {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[cfg(not(feature = "heap"))]
    unsafe {
        // Empirical tests show that this option results in the smallest RSS.
        mi_option_set_enabled(MiOption::PurgeDecommits, false)
    };
}
