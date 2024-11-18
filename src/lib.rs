use actuate::{composer::Composer, prelude::Compose};
use bevy::prelude::*;
use std::cell::RefCell;

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

pub fn compose(world: &World, wrap: NonSend<Runtime>) {
    let mut composer = wrap.composer.borrow_mut();
    composer.compose();
}
