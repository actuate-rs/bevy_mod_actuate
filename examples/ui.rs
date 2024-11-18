use actuate::prelude::*;
use bevy::prelude::*;
use bevy_mod_actuate::{compose, update, use_resource, Runtime};

#[derive(Debug, Resource)]
struct X(i32);

#[derive(Data)]
struct Ui;

impl Compose for Ui {
    fn compose(cx: Scope<Self>) -> impl Compose {
        let x = use_resource::<X>(&cx);
        dbg!(x.get());

        x.get_mut().update(|x| x.0 += 1);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_non_send_resource(Runtime::new(Ui))
        .insert_resource(X(0))
        .add_systems(Update, (compose, update))
        .run();
}
