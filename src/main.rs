#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

mod app;
mod grid;
mod rules;
mod storage;

use num_traits::*;
use xous::Message;

const SERVER_NAME: &str = "_Carse Automata_";
const APP_NAME: &str = "Carse Automata";

/// Opcodes for the application main loop.
#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
pub(crate) enum AppOp {
    Pump,
    Redraw,
    Rawkeys,
    FocusChange,
    Quit,
}

/// Pump rate in milliseconds. 100ms = 10 ticks/sec for simulation.
const PUMP_RATE_MS: usize = 100;

/// Opcodes for the pump thread.
#[derive(Debug, num_derive::FromPrimitive, num_derive::ToPrimitive)]
pub(crate) enum PumpOp {
    Run,
    Stop,
    Pump,
    Quit,
}

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("Carse Automata PID {}", xous::process::id());

    let xns = xous_names::XousNames::new().unwrap();
    let sid = xns.register_name(SERVER_NAME, None).expect("can't register server");

    let mut carse = app::CarseApp::new(sid);

    // Create pump thread
    let pump_sid = xous::create_server().unwrap();
    let cid_to_pump = xous::connect(pump_sid).unwrap();
    pump_thread(xous::connect(sid).unwrap(), pump_sid);

    let tt = ticktimer_server::Ticktimer::new().unwrap();

    let mut allow_redraw = true;
    let mut into_allow_redraw = false;

    loop {
        let msg = xous::receive_message(sid).unwrap();
        match FromPrimitive::from_usize(msg.body.id()) {
            Some(AppOp::Redraw) => {
                if allow_redraw {
                    carse.draw_focus();
                    carse.draw();
                }
            }
            Some(AppOp::Pump) => {
                if into_allow_redraw {
                    carse.draw_focus();
                    into_allow_redraw = false;
                }
                if allow_redraw {
                    carse.pump_tick(&tt);
                    carse.draw();
                }
                xous::return_scalar(msg.sender, 1).expect("couldn't ack pump");
            }
            Some(AppOp::Rawkeys) => xous::msg_scalar_unpack!(msg, k1, k2, k3, k4, {
                let keys = [
                    core::char::from_u32(k1 as u32).unwrap_or('\u{0000}'),
                    core::char::from_u32(k2 as u32).unwrap_or('\u{0000}'),
                    core::char::from_u32(k3 as u32).unwrap_or('\u{0000}'),
                    core::char::from_u32(k4 as u32).unwrap_or('\u{0000}'),
                ];
                carse.handle_keys(keys, &tt);
            }),
            Some(AppOp::FocusChange) => xous::msg_scalar_unpack!(msg, new_state_code, _, _, _, {
                let new_state = gam::FocusState::convert_focus_change(new_state_code);
                match new_state {
                    gam::FocusState::Background => {
                        allow_redraw = false;
                        carse.save_state();
                        xous::send_message(
                            cid_to_pump,
                            Message::new_scalar(PumpOp::Stop.to_usize().unwrap(), 0, 0, 0, 0),
                        ).ok();
                    }
                    gam::FocusState::Foreground => {
                        into_allow_redraw = true;
                        allow_redraw = true;
                        carse.needs_full_redraw = true;
                        xous::send_message(
                            cid_to_pump,
                            Message::new_scalar(PumpOp::Run.to_usize().unwrap(), 0, 0, 0, 0),
                        ).ok();
                    }
                }
            }),
            Some(AppOp::Quit) => xous::msg_blocking_scalar_unpack!(msg, _, _, _, _, {
                carse.save_state();
                xous::send_message(
                    cid_to_pump,
                    Message::new_blocking_scalar(PumpOp::Quit.to_usize().unwrap(), 0, 0, 0, 0),
                ).ok();
                unsafe { xous::disconnect(cid_to_pump).ok() };
                xous::return_scalar(msg.sender, 1).ok();
                break;
            }),
            _ => log::error!("unknown opcode: {:?}", msg),
        }
    }

    xns.unregister_server(sid).unwrap();
    xous::destroy_server(sid).unwrap();
    xous::terminate_process(0)
}

/// Pump thread: sends periodic Pump messages to the main thread.
/// Follows the ball app pattern exactly.
fn pump_thread(cid_to_main: xous::CID, pump_sid: xous::SID) {
    let _ = std::thread::spawn({
        let cid_to_main = cid_to_main;
        let sid = pump_sid;
        move || {
            let tt = ticktimer_server::Ticktimer::new().unwrap();
            let cid_to_self = xous::connect(sid).unwrap();
            let mut run = true;
            loop {
                let msg = xous::receive_message(sid).unwrap();
                match FromPrimitive::from_usize(msg.body.id()) {
                    Some(PumpOp::Run) => {
                        run = true;
                        xous::send_message(
                            cid_to_self,
                            Message::new_scalar(PumpOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                        ).ok();
                    }
                    Some(PumpOp::Stop) => run = false,
                    Some(PumpOp::Pump) => {
                        xous::send_message(
                            cid_to_main,
                            Message::new_blocking_scalar(AppOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                        ).ok();
                        if run {
                            tt.sleep_ms(PUMP_RATE_MS).unwrap();
                            xous::send_message(
                                cid_to_self,
                                Message::new_scalar(PumpOp::Pump.to_usize().unwrap(), 0, 0, 0, 0),
                            ).ok();
                        }
                    }
                    Some(PumpOp::Quit) => {
                        xous::return_scalar(msg.sender, 1).ok();
                        break;
                    }
                    _ => log::error!("pump: unknown opcode: {:?}", msg),
                }
            }
            xous::destroy_server(sid).ok();
        }
    });
}
