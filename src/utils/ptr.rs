#[inline]
pub(crate) unsafe fn voidp_to_ref<'a, T>(ptr: *mut core::ffi::c_void) -> &'a mut T {
  unsafe { &mut *ptr.cast() }
}
