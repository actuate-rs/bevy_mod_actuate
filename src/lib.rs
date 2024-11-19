//! # bevy_mod_actuate
//!
//! Declarative scenes and reactivity for [Bevy](https://crates.io/crates/bevy) powered by [Actuate](https://crates.io/crates/actuate).
//!
//! ```no_run
//! use actuate::prelude::*;
//! use bevy::prelude::*;
//! use bevy_mod_actuate::prelude::*;
//!
//! #[derive(Data)]
//! struct Timer;
//!
//! impl Compose for Timer {
//!     fn compose(cx: Scope<Self>) -> impl Compose {
//!         let time = use_resource::<Time>(&cx);
//!
//!         spawn(Text::new(format!("Elapsed: {:?}", time.elapsed())))
//!     }
//! }
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, ActuatePlugin))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2d::default());
//!
//!     commands.spawn(Composition::new(Timer));
//! }
//! ```

#![deny(missing_docs)]

use actuate::{
    composer::{Composer, Update, Updater},
    prelude::*,
};
use bevy::{
    app::Plugin,
    ecs::{component::StorageType, world::CommandQueue},
    prelude::{App, BuildChildren, Bundle, Component, Entity, Resource, World},
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

/// Prelude of common items.
pub mod prelude {
    pub use crate::{
        spawn, spawn_with, use_bundle, use_resource, ActuatePlugin, Composition, UseResource,
        UseWorld,
    };
}

/// Actuate plugin to run [`Composition`]s.
pub struct ActuatePlugin;

impl Plugin for ActuatePlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<Runtime>()
            .add_systems(bevy::prelude::Update, compose);
    }
}

struct Listener {
    is_changed_fn: Box<dyn FnMut(&World) -> bool>,
    fns: Vec<Box<dyn Fn()>>,
}

type UpdateFn = Box<dyn FnMut(&mut World)>;

struct Inner {
    world_ptr: *mut World,
    listeners: Vec<Box<dyn Fn()>>,
    resource_listeners: HashMap<TypeId, Listener>,
    updates: Vec<UpdateFn>,
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

struct RuntimeComposer {
    composer: Composer,
    guard: Option<RwLockWriteGuard<'static, ()>>,
}

struct Runtime {
    composers: RefCell<HashMap<Entity, RuntimeComposer>>,
    lock: Option<RwLockWriteGuard<'static, ()>>,
    tx: mpsc::Sender<Update>,
    rx: mpsc::Receiver<Update>,
}

impl Default for Runtime {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            composers: RefCell::new(HashMap::new()),
            lock: None,
            tx,
            rx,
        }
    }
}

/// Composition of some composable content.
pub struct Composition<C> {
    content: Option<C>,
}

impl<C> Composition<C>
where
    C: Compose + Send + Sync + 'static,
{
    /// Create a new composition from its content.
    pub fn new(content: C) -> Self {
        Self {
            content: Some(content),
        }
    }
}

impl<C> Component for Composition<C>
where
    C: Compose + Send + Sync + 'static,
{
    const STORAGE_TYPE: StorageType = StorageType::SparseSet;
    fn register_component_hooks(hooks: &mut bevy::ecs::component::ComponentHooks) {
        hooks.on_insert(|mut world, entity, _| {
            world.commands().queue(move |world: &mut World| {
                let content = world
                    .get_mut::<Composition<C>>(entity)
                    .unwrap()
                    .content
                    .take();

                let tx = world.non_send_resource::<Runtime>().tx.clone();

                world
                    .non_send_resource_mut::<Runtime>()
                    .composers
                    .borrow_mut()
                    .insert(
                        entity,
                        RuntimeComposer {
                            composer: Composer::with_updater(
                                content,
                                RuntimeUpdater { queue: tx },
                                tokio::runtime::Runtime::new().unwrap(),
                            ),
                            guard: None,
                        },
                    );
            });
        });
    }
}

fn compose(world: &mut World) {
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
    let mut composers = rt.composers.borrow_mut();
    for rt_composer in composers.values_mut() {
        rt_composer.guard = None;
        rt_composer.composer.compose();
    }
    drop(composers);

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

    let rt = &mut *world.non_send_resource_mut::<Runtime>();
    let mut composers = rt.composers.borrow_mut();
    for rt_composer in composers.values_mut() {
        let guard = rt_composer.composer.lock();
        let guard: RwLockWriteGuard<'static, ()> = unsafe { mem::transmute(guard) };
        rt_composer.guard = Some(guard);
    }
}

/// Hook for [`use_world`].
pub struct UseWorld<'a> {
    _marker: PhantomData<ScopeState<'a>>,
}

/// Use access to the current ECS world.
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
    /// Get a resource by its type.
    pub fn resource<R: Resource + Clone>(&self) -> R {
        unsafe {
            RuntimeContext::current()
                .world_mut()
                .resource::<R>()
                .clone()
        }
    }
}

/// Use a [`Resource`] from the ECS world.
///
/// Changing a resource tracked with this hook will cause the composable to re-compose.
pub fn use_resource<R>(cx: ScopeState) -> UseResource<R>
where
    R: Resource + Clone,
{
    let world = unsafe { RuntimeContext::current().world_mut() };

    let value = use_mut(cx, || world.resource::<R>().clone());

    let f: Box<dyn Fn()> = Box::new(move || {
        let new = world.resource::<R>().clone();
        value.update(move |value| *value = new);
    });
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
        value: value.as_ref(),
    }
}

/// Hook for [`use_resource`].
pub struct UseResource<'a, R> {
    value: Ref<'a, R>,
}

impl<R> Clone for UseResource<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for UseResource<'_, R> {}

impl<R> Deref for UseResource<'_, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

struct SpawnContext {
    parent_entity: Entity,
}

/// Use a spawned bundle.
///
/// `make_bundle` is called once to create the bundle.
pub fn use_bundle<B: Bundle>(cx: ScopeState, make_bundle: impl FnOnce() -> B) -> Entity {
    use_bundle_inner(cx, |world, cell| {
        let bundle = make_bundle();
        if let Some(entity) = cell {
            world.entity_mut(*entity).insert(bundle);
        } else {
            *cell = Some(world.spawn(bundle).id());
        }
    })
}

type SpawnFn = Arc<dyn Fn(&mut World, &mut Option<Entity>)>;

/// Create a [`Spawn`] composable that spawns the provided `bundle` when composed.
///
/// On re-composition, the spawned entity is updated to the latest provided value.
pub fn spawn<B>(bundle: B) -> Spawn
where
    B: Bundle + Clone,
{
    Spawn {
        spawn_fn: Arc::new(move |world, cell| {
            if let Some(entity) = cell {
                world.entity_mut(*entity).insert(bundle.clone());
            } else {
                *cell = Some(world.spawn(bundle.clone()).id())
            }
        }),
    }
}

/// Spawn composable.
///
/// See [`spawn`] for more information.
#[derive(Data)]
pub struct Spawn {
    spawn_fn: SpawnFn,
}

impl Compose for Spawn {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let spawn_cx = use_context::<SpawnContext>(&cx);

        let entity = use_bundle_inner(&cx, |world, entity| {
            (cx.me().spawn_fn)(world, entity);
        });

        use_ref(&cx, || {
            if let Ok(parent_entity) = spawn_cx.map(|cx| cx.parent_entity) {
                let world = unsafe { RuntimeContext::current().world_mut() };
                world.entity_mut(parent_entity).add_child(entity);
            }
        });
    }
}

/// Create a [`Spawn`] composable that spawns the provided `bundle` when composed, with some content as its children.
///
/// On re-composition, the spawned entity is updated to the latest provided value.
pub fn spawn_with<B, C>(bundle: B, content: C) -> SpawnWith<C>
where
    B: Bundle + Clone,
    C: Compose,
{
    SpawnWith {
        spawn_fn: Arc::new(move |world, cell| {
            if let Some(entity) = cell {
                world.entity_mut(*entity).insert(bundle.clone());
            } else {
                *cell = Some(world.spawn(bundle.clone()).id())
            }
        }),
        content,
    }
}

/// Spawn composable with content.
///
/// See [`spawn_with`] for more information.
#[derive(Data)]
pub struct SpawnWith<C> {
    spawn_fn: SpawnFn,
    content: C,
}

impl<C: Compose> Compose for SpawnWith<C> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let spawn_cx = use_context::<SpawnContext>(&cx);

        let entity = use_bundle_inner(&cx, |world, entity| {
            (cx.me().spawn_fn)(world, entity);
        });

        use_provider(&cx, || {
            if let Ok(parent_entity) = spawn_cx.map(|cx| cx.parent_entity) {
                let world = unsafe { RuntimeContext::current().world_mut() };
                world.entity_mut(parent_entity).add_child(entity);
            }

            SpawnContext {
                parent_entity: entity,
            }
        });

        Ref::map(cx.me(), |me| &me.content)
    }
}

fn use_bundle_inner(cx: ScopeState, spawn: impl FnOnce(&mut World, &mut Option<Entity>)) -> Entity {
    let mut f_cell = Some(spawn);
    let entity = *use_ref(cx, || {
        let world = unsafe { RuntimeContext::current().world_mut() };

        let mut cell = None;
        f_cell.take().unwrap()(world, &mut cell);
        cell.unwrap()
    });

    if let Some(f) = f_cell {
        let world = unsafe { RuntimeContext::current().world_mut() };
        f(world, &mut Some(entity));
    }

    use_drop(cx, move || {
        let world = unsafe { RuntimeContext::current().world_mut() };
        world.despawn(entity);
    });

    entity
}
