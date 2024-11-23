//! # bevy_mod_actuate
//!
//! Declarative scenes and reactivity for [Bevy](https://crates.io/crates/bevy) powered by [Actuate](https://crates.io/crates/actuate).
//!
//! ```no_run
//! use actuate::prelude::{*, Mut};
//! use bevy::prelude::*;
//! use bevy_mod_actuate::prelude::*;
//!
//! #[derive(Data)]
//! struct Timer;
//!
//! impl Compose for Timer {
//!     fn compose(cx: Scope<Self>) -> impl Compose {
//!         let current_time = use_mut(&cx, Time::default);
//!
//!         // Use the `Time` resource from the ECS world, updating the `current_time`.
//!         use_world(&cx, move |time: Res<Time>| Mut::set(current_time, *time));
//!
//!         // Spawn a `Text` component, updating it when this scope is re-composed.
//!         spawn(Text::new(format!("Elapsed: {:?}", current_time.elapsed())))
//!    }
//! }
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, ActuatePlugin::new()))
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
#![cfg_attr(docsrs, feature(doc_cfg))]

use actuate::{
    composer::{Composer, Executor, Update, Updater},
    prelude::*,
    use_callback,
};
use bevy::{
    app::Plugin,
    ecs::{
        component::{ComponentHooks, StorageType},
        system::{SystemParam, SystemState},
        world::CommandQueue,
    },
    prelude::{App, BuildChildren, Bundle, Component, Entity, World},
    utils::HashMap,
};
use slotmap::{DefaultKey, SlotMap};
use std::{
    any::TypeId,
    cell::RefCell,
    marker::PhantomData,
    mem, ptr,
    rc::Rc,
    sync::{mpsc, Arc},
};
use tokio::sync::RwLockWriteGuard;

/// Prelude of common items.
pub mod prelude {
    pub use crate::{
        spawn, spawn_with, use_bundle, use_world, ActuatePlugin, Composition, UseWorld,
    };
}

/// Actuate plugin to run [`Composition`]s.
pub struct ActuatePlugin {
    executor: Arc<dyn Executor + Send + Sync>,
}

#[cfg(feature = "rt")]
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
impl Default for ActuatePlugin {
    fn default() -> Self {
        let rt = tokio::runtime::Runtime::new().unwrap();
        Self {
            executor: Arc::new(rt),
        }
    }
}

impl ActuatePlugin {
    #[cfg(feature = "rt")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
    /// Create the default Actuate plugin.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new Actuate plugin with the provided executor.
    pub fn from_executor(executor: impl Executor + Send + Sync + 'static) -> Self {
        Self {
            executor: Arc::new(executor),
        }
    }
}

impl Plugin for ActuatePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = mpsc::channel();
        let rt = Runtime {
            composers: RefCell::new(HashMap::new()),
            lock: None,
            tx,
            rx,
            executor: self.executor.clone(),
        };

        app.insert_non_send_resource(rt)
            .add_systems(bevy::prelude::Update, compose);
    }
}

struct Listener {
    is_changed_fn: Box<dyn FnMut(&World) -> bool>,
    fns: Vec<Box<dyn Fn()>>,
}

type UpdateFn = Box<dyn FnMut(&mut World)>;

type WorldListenerFn = Rc<dyn Fn(&mut World)>;

struct Inner {
    world_ptr: *mut World,
    listeners: SlotMap<DefaultKey, WorldListenerFn>,
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
    executor: Arc<dyn Executor>,
}

/// Composition of some composable content.
pub struct Composition<C> {
    content: Option<C>,
    target: Option<Entity>,
}

impl<C> Composition<C>
where
    C: Compose + Send + Sync + 'static,
{
    /// Create a new composition from its content.
    pub fn new(content: C) -> Self {
        Self {
            content: Some(content),
            target: None,
        }
    }

    /// Get the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn target(&self) -> Option<Entity> {
        self.target
    }

    /// Set the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn set_target(&mut self, target: Option<Entity>) {
        self.target = target;
    }

    /// Set the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn with_target(mut self, target: Entity) -> Self {
        self.target = Some(target);
        self
    }
}

#[derive(Data)]
struct CompositionContent<C> {
    content: C,
    target: Entity,
}

impl<C: Compose> Compose for CompositionContent<C> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        use_provider(&cx, || SpawnContext {
            parent_entity: cx.me().target,
        });

        Ref::map(cx.me(), |me| &me.content)
    }
}

impl<C> Component for Composition<C>
where
    C: Compose + Send + Sync + 'static,
{
    const STORAGE_TYPE: StorageType = StorageType::SparseSet;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_insert(|mut world, entity, _| {
            world.commands().queue(move |world: &mut World| {
                let mut composition = world.get_mut::<Composition<C>>(entity).unwrap();

                let content = composition.content.take().unwrap();

                let target = composition.target.unwrap_or(entity);

                let tx = world.non_send_resource::<Runtime>().tx.clone();

                let rt = world.non_send_resource_mut::<Runtime>();
                let executor = rt.executor.clone();
                rt.composers.borrow_mut().insert(
                    entity,
                    RuntimeComposer {
                        composer: Composer::with_updater(
                            CompositionContent { content, target },
                            RuntimeUpdater { queue: tx },
                            executor,
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
                listeners: SlotMap::new(),
                resource_listeners: HashMap::new(),
                updates: Vec::new(),
                commands: CommandQueue::default(),
            })),
        });

        for f in runtime_cx.inner.borrow().listeners.values() {
            f(world)
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

/// A function that takes a [`SystemParam`] as input.
pub trait SystemParamFunction<Marker> {
    /// The [`SystemParam`].
    type Param: SystemParam + 'static;

    /// Run the function with the provided [`SystemParam`]'s item.
    fn run(&self, param: <Self::Param as SystemParam>::Item<'_, '_>);
}

macro_rules! impl_system_param_fn {
    ($($t:tt),*) => {
        impl<$($t: SystemParam + 'static,)* F: Fn($($t),*) + Fn($($t::Item<'_, '_>),*)> SystemParamFunction<fn($($t),*)> for F {
            type Param = ($($t,)*);

            fn run(&self, param: <Self::Param as SystemParam>::Item<'_, '_>) {
                #[allow(non_snake_case)]
                let ($($t,)*) = param;
                self($($t,)*)
            }
        }
    };
}

impl_system_param_fn!(T1);
impl_system_param_fn!(T1, T2);
impl_system_param_fn!(T1, T2, T3);
impl_system_param_fn!(T1, T2, T3, T4);
impl_system_param_fn!(T1, T2, T3, T4, T5);
impl_system_param_fn!(T1, T2, T3, T4, T5, T6);
impl_system_param_fn!(T1, T2, T3, T4, T5, T6, T7);
impl_system_param_fn!(T1, T2, T3, T4, T5, T6, T7, T8);

/// Use a [`SystemParam`] from the ECS world.
///
/// `with_param` will be called on every frame with the latest query.
///
/// Change detection is implemented as a traditional system parameter.
pub fn use_world<'a, Marker, F>(cx: ScopeState<'a>, with_param: F)
where
    F: SystemParamFunction<Marker> + 'a,
{
    let system_state_cell = use_ref(cx, || RefCell::new(None));

    let f: Rc<dyn Fn(&'static mut World)> = use_callback(cx, move |world: &'static mut World| {
        let mut system_state_cell = system_state_cell.borrow_mut();
        let system_state =
            system_state_cell.get_or_insert_with(|| SystemState::<F::Param>::new(world));
        let query = system_state.get_mut(world);
        with_param.run(query)
    })
    .clone();
    let f: Rc<dyn Fn(&mut World)> = unsafe { mem::transmute(f) };

    let key = *use_ref(cx, || {
        RuntimeContext::current()
            .inner
            .borrow_mut()
            .listeners
            .insert(f)
    });

    use_drop(cx, move || {
        RuntimeContext::current()
            .inner
            .borrow_mut()
            .listeners
            .remove(key);
    });
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
pub fn spawn<B>(bundle: B) -> SpawnWith<()>
where
    B: Bundle + Clone,
{
    spawn_with(bundle, ())
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
        target: None,
    }
}

/// Spawn composable with content.
///
/// See [`spawn_with`] for more information.
#[derive(Data)]
#[must_use = "Composables do nothing unless composed with `actuate::run` or returned from other composables"]
pub struct SpawnWith<C> {
    spawn_fn: SpawnFn,
    content: C,
    target: Option<Entity>,
}

impl<C> SpawnWith<C> {
    /// Get the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn target(&self) -> Option<Entity> {
        self.target
    }

    /// Set the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn set_target(&mut self, target: Option<Entity>) {
        self.target = target;
    }

    /// Set the target entity to spawn the composition into.
    ///
    /// If `None`, this will use the composition's parent (if any).
    pub fn with_target(mut self, target: Entity) -> Self {
        self.target = Some(target);
        self
    }
}

impl<C: Compose> Compose for SpawnWith<C> {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let spawn_cx = use_context::<SpawnContext>(&cx);

        let entity = use_bundle_inner(&cx, |world, entity| {
            if let Some(target) = cx.me().target {
                *entity = Some(target);
            }

            (cx.me().spawn_fn)(world, entity);
        });

        use_provider(&cx, || {
            if cx.me().target.is_none() {
                if let Ok(parent_entity) = spawn_cx.map(|cx| cx.parent_entity) {
                    let world = unsafe { RuntimeContext::current().world_mut() };
                    world.entity_mut(parent_entity).add_child(entity);
                }
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
        world.try_despawn(entity);
    });

    entity
}
