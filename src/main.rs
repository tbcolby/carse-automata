#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

//! Carse Automata for Precursor. The rules, record formats and input timing
//! live in `carse-core` (host-tested). This crate registers with GAM, owns the
//! pump and the PDDB, and draws.

mod app;

use num_traits::{FromPrimitive, ToPrimitive};
use precursor_appkit::{app as kit, keys, pump, rng, store};

/// xous-names server name. Any unique string; NOT the GAM app name.
const SERVER_NAME: &str = "_Carse Automata_";

/// Pump rate: 100 ms, the simulation step. Generations actually run are
/// computed from ticktimer deltas, so a late tick doesn't slow time down.
const PUMP_MS: usize = 100;

#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
pub(crate) enum AppOp {
    Redraw = 0,
    Rawkeys,
    FocusChange,
    /// Blocking scalar from the pump; answered with `pump::ack`.
    Tick,
    BasisChanged,
    Quit,
}

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("carse PID is {}", xous::process::id());

    let xns = xous_names::XousNames::new().unwrap();
    let sid = xns.register_name(SERVER_NAME, None).expect("can't register server");

    // GAM app name comes from apps/manifest.json key "carse".
    let reg = kit::register(
        &xns,
        sid,
        gam::APP_NAME_CARSE,
        gam::UxType::Framebuffer,
        kit::Opcodes {
            redraw: AppOp::Redraw.to_u32().unwrap(),
            rawkeys: Some(AppOp::Rawkeys.to_u32().unwrap()),
            focus_change: Some(AppOp::FocusChange.to_u32().unwrap()),
            got_input: None,
        },
    );

    let trng = rng::Trng::new(&xns).expect("can't connect to TRNG");
    // Blocks until the PDDB is mounted (the user has entered the PIN) instead of
    // calling try_mount(), which can pop up a PIN prompt at boot.
    let store = store::Store::open(app::DICT);
    store::watch_bases(xous::connect(sid).unwrap(), AppOp::BasisChanged.to_usize().unwrap());
    let pump = pump::Pump::spawn(xous::connect(sid).unwrap(), AppOp::Tick.to_usize().unwrap(), PUMP_MS);

    let mut app = app::App::new(reg.screen, store, trng);

    loop {
        let msg = xous::receive_message(sid).unwrap();
        match FromPrimitive::from_usize(msg.body.id()) {
            Some(AppOp::Redraw) => app.redraw(),
            Some(AppOp::Rawkeys) => xous::msg_scalar_unpack!(msg, k1, k2, k3, k4, {
                let pressed: Vec<keys::Key> = keys::decode_rawkeys([k1, k2, k3, k4]).collect();
                if app.on_keys(&pressed) == app::Action::Leave {
                    kit::leave(app.screen());
                }
            }),
            Some(AppOp::FocusChange) => xous::msg_scalar_unpack!(msg, code, _, _, _, {
                match kit::focus_state(code) {
                    gam::FocusState::Foreground => {
                        app.on_foreground();
                        pump.run();
                    }
                    gam::FocusState::Background => {
                        pump.stop();
                        app.on_background();
                    }
                }
            }),
            Some(AppOp::Tick) => {
                app.on_tick();
                pump::ack(&msg);
            }
            Some(AppOp::BasisChanged) => app.reload(),
            Some(AppOp::Quit) => break,
            None => log::warn!("unknown opcode {:?}", msg.body.id()),
        }
    }
    app.on_background();
    pump.quit();
    kit::shutdown(&xns, sid)
}
