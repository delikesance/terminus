//! Resume-from-sleep notifications on Linux.
//!
//! Wayland (and X11) have no "the machine woke up" event, but systemd-logind
//! emits `PrepareForSleep(false)` on the system bus once resume completes.
//! After a suspend/hibernate the GPU may hand back blank textures while the
//! glyph caches still believe their slots are uploaded, so the app listens
//! here and rebuilds them (`RioEvent::SystemResumed`).

use rio_backend::event::{EventProxy, RioEvent, RioEventType, WindowId};

const LOGIND_DESTINATION: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const LOGIND_MANAGER: &str = "org.freedesktop.login1.Manager";

/// Calls `on_resume` for every `PrepareForSleep(false)` in `signals`
/// (`true` means the machine is about to sleep, `false` that it woke up).
fn forward_resumes(signals: impl Iterator<Item = bool>, mut on_resume: impl FnMut()) {
    for about_to_sleep in signals {
        if !about_to_sleep {
            on_resume();
        }
    }
}

/// Listens for logind resume signals on a background thread and sends
/// `RioEvent::SystemResumed` through `event_proxy`. Without a system bus
/// or logind (containers, non-systemd distros) it logs and gives up.
pub fn start_resume_monitor(event_proxy: EventProxy) {
    let spawned = std::thread::Builder::new()
        .name("terminus-resume-monitor".into())
        .spawn(move || {
            if let Err(err) = run(&event_proxy) {
                tracing::warn!("resume monitor exited: {err}");
            }
        });
    if let Err(err) = spawned {
        tracing::warn!("failed to spawn resume monitor: {err}");
    }
}

fn run(event_proxy: &EventProxy) -> Result<(), zbus::Error> {
    let connection = zbus::blocking::Connection::system()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        LOGIND_DESTINATION,
        LOGIND_PATH,
        LOGIND_MANAGER,
    )?;
    let signals = proxy
        .receive_signal("PrepareForSleep")?
        .filter_map(|message| message.body().deserialize::<bool>().ok());
    forward_resumes(signals, || {
        event_proxy.send_event(
            RioEventType::Rio(RioEvent::SystemResumed),
            WindowId::from(0),
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::forward_resumes;

    fn resumes(signals: &[bool]) -> usize {
        let mut count = 0;
        forward_resumes(signals.iter().copied(), || count += 1);
        count
    }

    #[test]
    fn going_to_sleep_is_not_a_resume() {
        assert_eq!(resumes(&[true]), 0);
    }

    #[test]
    fn each_wake_up_is_one_resume() {
        assert_eq!(resumes(&[true, false, true, false]), 2);
    }

    #[test]
    fn no_signal_no_resume() {
        assert_eq!(resumes(&[]), 0);
    }
}
