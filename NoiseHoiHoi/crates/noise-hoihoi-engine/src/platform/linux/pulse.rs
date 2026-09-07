use std::{
    cell::RefCell,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

use libpulse_binding::{
    callbacks::ListResult,
    context::{Context, FlagSet, State},
    def::INVALID_INDEX,
    mainloop::standard::{IterateResult, Mainloop},
    operation::{Operation, State as OperationState},
};

use crate::{AudioDevice, EngineError};

pub(super) const SINK_NAME: &str = "noise_hoihoi_output";
pub(super) const SOURCE_NAME: &str = "noise_hoihoi_microphone";
const TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn error(message: impl std::fmt::Display) -> EngineError {
    EngineError::Start(format!("Linux audio: {message}"))
}

// All PulseAudio objects live on one thread and use the nonblocking mainloop.
// Context must be destroyed before its mainloop.
pub(super) struct Connection {
    pub context: Context,
    mainloop: Mainloop,
    modules: Vec<u32>,
}

impl Connection {
    pub fn new() -> Result<Self, EngineError> {
        let mainloop = Mainloop::new().ok_or_else(|| error("cannot create audio mainloop"))?;
        let context = Context::new(&mainloop, "NoiseHoiHoi")
            .ok_or_else(|| error("cannot create audio context"))?;
        let mut connection = Self {
            context,
            mainloop,
            modules: Vec::new(),
        };
        connection
            .context
            .connect(None, FlagSet::NOAUTOSPAWN, None)
            .map_err(error)?;
        let deadline = Instant::now() + TIMEOUT;
        while connection.context.get_state() != State::Ready {
            connection.tick()?;
            if Instant::now() >= deadline {
                return Err(error(
                    "connection timed out; start PipeWire with pipewire-pulse or PulseAudio",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(connection)
    }

    pub fn tick(&mut self) -> Result<(), EngineError> {
        if !matches!(self.mainloop.iterate(false), IterateResult::Success(_))
            || matches!(self.context.get_state(), State::Failed | State::Terminated)
        {
            return Err(error(
                "audio server disconnected; start PipeWire with pipewire-pulse or PulseAudio, then Retry",
            ));
        }
        Ok(())
    }

    fn wait<T: ?Sized>(&mut self, mut operation: Operation<T>) -> Result<(), EngineError> {
        let deadline = Instant::now() + TIMEOUT;
        while operation.get_state() == OperationState::Running {
            if let Err(err) = self.tick() {
                operation.cancel();
                return Err(err);
            }
            if Instant::now() >= deadline {
                operation.cancel();
                return Err(error("audio server operation timed out"));
            }
            thread::sleep(Duration::from_millis(1));
        }
        if operation.get_state() != OperationState::Done {
            return Err(error("audio server operation was cancelled"));
        }
        Ok(())
    }

    pub fn inputs(&mut self) -> Result<Vec<AudioDevice>, EngineError> {
        let default = Rc::new(RefCell::new(None));
        let result = Rc::clone(&default);
        let op = self.context.introspect().get_server_info(move |info| {
            *result.borrow_mut() = info.default_source_name.as_ref().map(ToString::to_string);
        });
        self.wait(op)?;
        let devices = Rc::new(RefCell::new(Vec::new()));
        let result = Rc::clone(&devices);
        let failed = Rc::new(RefCell::new(false));
        let callback_failed = Rc::clone(&failed);
        let op = self
            .context
            .introspect()
            .get_source_info_list(move |item| match item {
                ListResult::Item(info) => {
                    let Some(name) = info.name.as_deref() else {
                        return;
                    };
                    if !is_input(name, info.monitor_of_sink.is_some()) {
                        return;
                    }
                    result.borrow_mut().push(AudioDevice {
                        id: name.to_owned(),
                        name: info.description.as_deref().unwrap_or(name).to_owned(),
                        is_default: default.borrow().as_deref() == Some(name),
                    });
                }
                ListResult::Error => *callback_failed.borrow_mut() = true,
                ListResult::End => {}
            });
        self.wait(op)?;
        if *failed.borrow() {
            return Err(error("cannot enumerate microphones"));
        }
        let mut devices = std::mem::take(&mut *devices.borrow_mut());
        devices.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(devices)
    }

    pub fn create_microphone(&mut self) -> Result<(), EngineError> {
        // The session lock is already held. Recover only modules with our exact
        // type and arguments, including modules from the initial v0.6 build.
        self.recover_abandoned_modules()?;
        // Refuse endpoints that belong to any other configuration.
        let occupied = Rc::new(RefCell::new(false));
        let found = Rc::clone(&occupied);
        let op = self.context.introspect().get_sink_info_list(move |item| {
            if let ListResult::Item(info) = item
                && info.name.as_deref() == Some(SINK_NAME)
            {
                *found.borrow_mut() = true;
            }
        });
        self.wait(op)?;
        let found = Rc::clone(&occupied);
        let op = self.context.introspect().get_source_info_list(move |item| {
            if let ListResult::Item(info) = item
                && info.name.as_deref() == Some(SOURCE_NAME)
            {
                *found.borrow_mut() = true;
            }
        });
        self.wait(op)?;
        if *occupied.borrow() {
            return Err(error(
                "NoiseHoiHoi virtual microphone already exists; close the other instance or remove stale NoiseHoiHoi modules (see Linux README)",
            ));
        }
        self.load("module-null-sink", &sink_arguments())?;
        self.load("module-remap-source", &source_arguments())
    }

    fn recover_abandoned_modules(&mut self) -> Result<(), EngineError> {
        let owned = Rc::new(RefCell::new(Vec::new()));
        let found = Rc::clone(&owned);
        let failed = Rc::new(RefCell::new(false));
        let callback_failed = Rc::clone(&failed);
        let op = self
            .context
            .introspect()
            .get_module_info_list(move |item| match item {
                ListResult::Item(info) => {
                    let name = info.name.as_deref().unwrap_or_default();
                    let args = info.argument.as_deref().unwrap_or_default();
                    if (name == "module-null-sink" && args == sink_arguments())
                        || (name == "module-remap-source" && args == source_arguments())
                    {
                        found
                            .borrow_mut()
                            .push((name == "module-null-sink", info.index));
                    }
                }
                ListResult::Error => *callback_failed.borrow_mut() = true,
                ListResult::End => {}
            });
        self.wait(op)?;
        if *failed.borrow() {
            return Err(error("cannot inspect abandoned virtual microphone modules"));
        }
        let mut owned = std::mem::take(&mut *owned.borrow_mut());
        owned.sort_unstable(); // Remove the source before its master sink.
        for (_, index) in owned {
            let success = Rc::new(RefCell::new(false));
            let result = Rc::clone(&success);
            let op = self
                .context
                .introspect()
                .unload_module(index, move |ok| *result.borrow_mut() = ok);
            self.wait(op)?;
            if !*success.borrow() {
                return Err(error(
                    "cannot remove an abandoned virtual microphone module; Retry",
                ));
            }
        }
        Ok(())
    }

    fn load(&mut self, module: &str, args: &str) -> Result<(), EngineError> {
        let index = Rc::new(RefCell::new(INVALID_INDEX));
        let result = Rc::clone(&index);
        let op = self
            .context
            .introspect()
            .load_module(module, args, move |id| *result.borrow_mut() = id);
        self.wait(op)?;
        let index = *index.borrow();
        if index == INVALID_INDEX {
            return Err(error(format!(
                "cannot load {module}; the audio server must allow virtual microphone modules"
            )));
        }
        self.modules.push(index);
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        while let Some(index) = self.modules.pop() {
            if self.context.get_state() != State::Ready {
                break;
            }
            let op = self.context.introspect().unload_module(index, |_| {});
            let _ = self.wait(op);
        }
        self.context.disconnect();
    }
}

fn sink_arguments() -> String {
    format!(
        "sink_name={SINK_NAME} rate=48000 channels=1 channel_map=mono sink_properties=device.description=NoiseHoiHoi_Output"
    )
}

fn source_arguments() -> String {
    format!(
        "master={SINK_NAME}.monitor source_name={SOURCE_NAME} channels=1 channel_map=mono master_channel_map=mono source_properties=\"device.description='{}'\"",
        crate::OUTPUT_MICROPHONE_NAME
    )
}

fn is_input(name: &str, is_monitor: bool) -> bool {
    !is_monitor && name != SOURCE_NAME && !name.starts_with("noise_hoihoi_")
}

#[cfg(test)]
mod tests {
    use super::is_input;

    #[test]
    fn excludes_monitor_and_own_virtual_sources() {
        assert!(is_input("alsa_input.usb_microphone", false));
        assert!(!is_input("alsa_output.monitor", true));
        assert!(!is_input("noise_hoihoi_microphone", false));
        assert!(!is_input("noise_hoihoi_output.monitor", false));
    }
}
