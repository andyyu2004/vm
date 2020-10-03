use super::Gc;
use rustc_hash::FxHashSet;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

#[derive(Default, Debug)]
pub struct GCStateMap(Inner);

// we can't use NonNull<dyn Trace> as this messes up equality (because of vtable ptr)
type Inner = FxHashSet<NonNull<()>>;

impl GCStateMap {
    pub fn mark_gc_ptr<T>(&mut self, gc: &Gc<T>)
    where
        T: Trace + 'static,
    {
        self.0.insert(gc.ptr.cast());
    }
}

impl Deref for GCStateMap {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GCStateMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait Trace {
    fn mark(&self, map: &mut GCStateMap) {
        let _ = map;
    }
}

impl Trace for String {
}

impl<T> Trace for &T
where
    T: Trace,
{
    fn mark(&self, map: &mut GCStateMap) {
        Trace::mark(*self, map)
    }
}

impl<T> Trace for NonNull<T>
where
    T: Trace,
{
    fn mark(&self, map: &mut GCStateMap) {
        unsafe { self.as_ref() }.mark(map)
    }
}

impl<T> Trace for Vec<T>
where
    T: Trace,
{
    fn mark(&self, map: &mut GCStateMap) {
        self.iter().for_each(|t| t.mark(map))
    }
}