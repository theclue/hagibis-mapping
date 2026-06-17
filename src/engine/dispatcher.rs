use crate::backend::Injector;
use crate::config::Config;
use crate::engine::consumer::ConsumerHandler;
use crate::engine::key_mapper::ConfigKeyMapper;
use crate::engine::keyboard::KeyboardHandler;
use crate::error::Error;
use crate::hid::Report;
use crate::log_debug;

/// Top-level dispatcher: routes a parsed [`Report`] to the appropriate
/// handler and injects remapped keys.
pub struct Dispatcher {
    keyboard: KeyboardHandler,
    consumer: ConsumerHandler,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            keyboard: KeyboardHandler::new(),
            consumer: ConsumerHandler::new(),
        }
    }

    pub fn dispatch(
        &mut self,
        report: &Report,
        config: &Config,
        focused_app_id: Option<&str>,
        injector: &dyn Injector,
    ) -> Result<(), Error> {
        let mapping = ConfigKeyMapper::resolve(config, focused_app_id);

        match report {
            Report::Keyboard(kb) => {
                log_debug!("dispatch", "keyboard mod=0x{:02X} keys={:?}", kb.modifier, kb.keycodes);
                self.keyboard.handle(kb, &mapping, injector)?
            }
            Report::Consumer(c) => {
                log_debug!("dispatch", "consumer bits=0x{:02X}", c.bits.0);
                self.consumer.handle(c, &mapping, injector)?
            }
            _ => {}
        }
        Ok(())
    }
}
