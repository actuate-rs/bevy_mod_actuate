use actuate::{
    composer::{Composer, Update, Updater},
    prelude::*,
};
use bevy::{
    ecs::world::CommandQueue,
    prelude::{BuildChildren, Bundle, Entity, Resource, World},
    utils::HashMap,
};
use std::{
    any::TypeId,
    cell::RefCell,
    marker::PhantomData,
    mem,
    ops::Deref,
    ptr,
    rc::Rc,
    sync::{mpsc, Arc},
};
use tokio::sync::RwLockWriteGuard;

struct Listener {
    is_changed_fn: Box<dyn FnMut(&World) -> bool>,
    fns: Vec<Box<dyn Fn()>>,
}

struct Inner {
    world_ptr: *mut World,
    listeners: Vec<Box<dyn Fn()>>,
    resource_listeners: HashMap<TypeId, Listener>,
    updates: Vec<Box<dyn FnMut(&mut World)>>,
    commands: CommandQueue,
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

    unsafe fn world_mut(&self) -> &'static mut World {
        &mut *self.inner.borrow().world_ptr
    }
}

thread_local! {
    static RUNTIME_CONTEXT: RefCell<Option<RuntimeContext>> = const { RefCell::new(None) };
}

struct RuntimeUpdater {
    queue: mpsc::Sender<Update>,
}

impl Updater for RuntimeUpdater {
    fn update(&self, update: Update) {
        self.queue.send(update).unwrap();
    }
}

unsafe impl Send for RuntimeUpdater {}

unsafe impl Sync for RuntimeUpdater {}

pub struct Runtime {
    composer: RefCell<Composer>,
    lock: Option<RwLockWriteGuard<'static, ()>>,
    rx: mpsc::Receiver<Update>,
}

impl Runtime {
    pub fn new(content: impl Compose + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            composer: RefCell::new(Composer::with_updater(
                content,
                RuntimeUpdater { queue: tx },
                tokio::runtime::Runtime::new().unwrap(),
            )),
            lock: None,
            rx,
        }
    }
}

pub fn compose(world: &mut World) {
    let mut rt = world.non_send_resource_mut::<Runtime>();
    rt.lock = None;

    RUNTIME_CONTEXT.with(|runtime_cx| {
        let mut cell = runtime_cx.borrow_mut();
        let runtime_cx = cell.get_or_insert_with(|| RuntimeContext {
            inner: Rc::new(RefCell::new(Inner {
                world_ptr: ptr::null_mut(),
                listeners: Vec::new(),
                resource_listeners: HashMap::new(),
                updates: Vec::new(),
                commands: CommandQueue::default(),
            })),
        });

        for f in &runtime_cx.inner.borrow_mut().listeners {
            f()
        }
        runtime_cx.inner.borrow_mut().listeners.clear();

        for listener in runtime_cx
            .inner
            .borrow_mut()
            .resource_listeners
            .values_mut()
        {
            if (listener.is_changed_fn)(world) {
                for f in &listener.fns {
                    f();
                }
                listener.fns.clear();
            }
        }

        runtime_cx.inner.borrow_mut().world_ptr = world as *mut World;
    });

    let rt = world.non_send_resource_mut::<Runtime>();
    let mut composer = rt.composer.borrow_mut();
    composer.compose();
    drop(composer);

    while let Ok(update) = rt.rx.try_recv() {
        unsafe { update.apply() }
    }

    {
        world.increment_change_tick();
        let rt_cx = RuntimeContext::current();
        let mut rt = rt_cx.inner.borrow_mut();
        for f in &mut rt.updates {
            f(world);
        }

        rt.updates.clear();

        rt.commands.apply(world);
    }

    let mut rt = world.non_send_resource_mut::<Runtime>();
    let composer = rt.composer.borrow_mut();
    let lock = composer.lock();
    let lock: RwLockWriteGuard<'static, ()> = unsafe { mem::transmute(lock) };
    drop(composer);
    rt.lock = Some(lock);
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
        unsafe { RuntimeContext::current().world_mut().resource() }
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
    pub fn update(self, f: impl FnOnce(&mut T) + 'static) {
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

impl<T> Clone for ResMut<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ResMut<'_, T> {}

impl<T> Deref for ResMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.resource
    }
}

pub fn use_resource<R: Resource>(cx: ScopeState) -> UseResource<R> {
    let f: Box<dyn Fn()> = Box::new(|| cx.set_changed());
    let f: Box<dyn Fn()> = unsafe { mem::transmute(f) };

    let rt_cx = RuntimeContext::current();
    let mut rt = rt_cx.inner.borrow_mut();

    if let Some(listener) = rt.resource_listeners.get_mut(&TypeId::of::<R>()) {
        listener.fns.push(f);
    } else {
        let mut cell = None;
        rt.resource_listeners.insert(
            TypeId::of::<R>(),
            Listener {
                is_changed_fn: Box::new(move |world| {
                    let current_tick = world.read_change_tick();
                    world.last_change_tick();

                    let last_changed_tick = world
                        .get_resource_change_ticks::<R>()
                        .unwrap()
                        .last_changed_tick();

                    if let Some(ref mut tick) = cell {
                        if last_changed_tick.is_newer_than(*tick, current_tick) {
                            *tick = current_tick;
                            true
                        } else {
                            false
                        }
                    } else {
                        cell = Some(current_tick);
                        true
                    }
                }),
                fns: vec![f],
            },
        );
    }

    UseResource {
        _marker: PhantomData,
    }
}

pub struct UseResource<'a, R> {
    _marker: PhantomData<fn(&'a World) -> &'a R>,
}

impl<R: Resource> UseResource<'_, R> {
    pub fn get(&self) -> &R {
        let world = unsafe { RuntimeContext::current().world_mut() };
        world.resource::<R>()
    }

    pub fn get_mut(&self) -> ResMut<'_, R> {
        let resource = self.get();
        ResMut { resource }
    }
}

impl<R> Clone for UseResource<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for UseResource<'_, R> {}

struct SpawnContext {
    parent_entity: Entity,
}

pub fn spawn<'a, B, C>(make_bundle: impl Fn() -> B + 'a, content: C) -> Spawn<'a, C>
where
    B: Bundle,
    C: Compose,
{
    Spawn {
        f: Arc::new(move |world| {
            let bundle = make_bundle();
            world.spawn(bundle).id()
        }),
        content,
    }
}

pub struct Spawn<'a, C> {
    f: Arc<dyn Fn(&mut World) -> Entity + 'a>,
    content: C,
}

unsafe impl<C: Data> Data for Spawn<'_, C> {
    type Id = Spawn<'static, C::Id>;
}

impl<C: Compose> Compose for Spawn<'_, C> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let entity = *use_ref(&cx, || {
            let world = unsafe { RuntimeContext::current().world_mut() };

            let parent_entity = use_context::<SpawnContext>(&cx)
                .ok()
                .map(|cx| cx.parent_entity);

            let entity = (cx.me().f)(world);

            if let Some(parent_entity) = parent_entity {
                world.entity_mut(parent_entity).add_child(entity);
            }

            entity
        });

        use_provider(&cx, || SpawnContext {
            parent_entity: entity,
        });

        use_drop(&cx, move || {
            let world = unsafe { RuntimeContext::current().world_mut() };
            world.despawn(entity);
        });

        Ref::map(cx.me(), |me| &me.content)
    }
}
