use actuate::{composer::Composer, prelude::Compose, ScopeState};
use bevy::prelude::*;
use std::{cell::RefCell, marker::PhantomData, mem, ops::Deref, ptr, rc::Rc};

struct Inner {
    world_ptr: *const World,
    listeners: Vec<Box<dyn Fn()>>,
    updates: Vec<Box<dyn FnMut(&mut World)>>,
}

#[derive(Clone)]
struct RuntimeContext {
    inner: Rc<RefCell<Inner>>,
}

impl RuntimeContext {
    fn current() -> Self {
        RUNTIME_CONTEXT.with(|cell| {
            let cell_ref = cell.borrow();
            let Some(rt) = cell_ref.as_ref() else {
                panic!("Must be called from within a composable.")
            };
            rt.clone()
        })
    }

    unsafe fn world(&self) -> &'static World {
        &*self.inner.borrow().world_ptr
    }
}

thread_local! {
    static RUNTIME_CONTEXT: RefCell<Option<RuntimeContext>> = const { RefCell::new(None) };
}

pub struct Runtime {
    composer: RefCell<Composer>,
}

impl Runtime {
    pub fn new(content: impl Compose + 'static) -> Self {
        Self {
            composer: RefCell::new(Composer::new(content)),
        }
    }
}

// TODO lock compose during ECS updates to prevent unsound async tasks
pub fn compose(world: &World, wrap: NonSend<Runtime>) {
    RUNTIME_CONTEXT.with(|runtime_cx| {
        let mut cell = runtime_cx.borrow_mut();
        let runtime_cx = cell.get_or_insert_with(|| RuntimeContext {
            inner: Rc::new(RefCell::new(Inner {
                world_ptr: ptr::null(),
                listeners: Vec::new(),
                updates: Vec::new(),
            })),
        });

        for f in &runtime_cx.inner.borrow_mut().listeners {
            f()
        }
        runtime_cx.inner.borrow_mut().listeners.clear();

        runtime_cx.inner.borrow_mut().world_ptr = world as *const World;
    });

    let mut composer = wrap.composer.borrow_mut();
    composer.compose();
}

pub fn update(world: &mut World) {
    let rt_cx = RuntimeContext::current();
    let mut rt = rt_cx.inner.borrow_mut();
    for f in &mut rt.updates {
        f(world);
    }
    rt.updates.clear();
}

pub struct UseWorld<'a> {
    _marker: PhantomData<ScopeState<'a>>,
}

pub fn use_world(cx: ScopeState) -> UseWorld {
    let f: Box<dyn Fn()> = Box::new(move || {
        cx.set_changed();
    });
    let f: Box<dyn Fn()> = unsafe { mem::transmute(f) };

    RuntimeContext::current()
        .inner
        .borrow_mut()
        .listeners
        .push(f);

    UseWorld {
        _marker: PhantomData,
    }
}

impl UseWorld<'_> {
    pub fn resource<T: Resource>(&self) -> &T {
        unsafe { RuntimeContext::current().world().resource() }
    }

    pub fn resource_mut<T: Resource>(&self) -> ResMut<T> {
        let resource = self.resource();
        ResMut { resource }
    }
}

pub struct ResMut<'a, T> {
    resource: &'a T,
}

impl<T: Resource> ResMut<'_, T> {
    pub fn update(&mut self, f: impl FnOnce(&mut T) + 'static) {
        let mut f_cell = Some(f);
        RuntimeContext::current()
            .inner
            .borrow_mut()
            .updates
            .push(Box::new(move |world| {
                let f = f_cell.take().unwrap();
                f(&mut world.resource_mut());
            }));
    }
}

impl<T> Deref for ResMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.resource
    }
}
