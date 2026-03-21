use gtk4::prelude::*;
use gtk4::{Button, EventControllerScroll, EventControllerScrollFlags};
use pulse::mainloop::standard::Mainloop;
use pulse::context::{Context, FlagSet as ContextFlagSet};
use pulse::context::subscribe::{Facility, InterestMaskSet};
use std::rc::Rc;
use std::cell::RefCell;

pub fn init(container: &gtk4::Box) {
    let btn = Button::builder()
        .label(" ...%")
        .build();
    btn.set_widget_name("volume-btn");
    container.append(&btn);

    let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    btn.add_controller(scroll.clone());

    scroll.connect_scroll(move |_, _, dy| {
        if dy < 0.0 {
            let _ = std::process::Command::new("pactl")
                .arg("set-sink-volume")
                .arg("@DEFAULT_SINK@")
                .arg("+5%")
                .spawn();
        } else if dy > 0.0 {
            let _ = std::process::Command::new("pactl")
                .arg("set-sink-volume")
                .arg("@DEFAULT_SINK@")
                .arg("-5%")
                .spawn();
        }
        glib::Propagation::Stop
    });

    btn.connect_clicked(|_| {
        let _ = std::process::Command::new("pactl")
            .arg("set-sink-mute")
            .arg("@DEFAULT_SINK@")
            .arg("toggle")
            .spawn();
    });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, i32)>();
    let b = btn.clone();
    gtk4::glib::MainContext::default().spawn_local(async move {
        while let Some((vol, perc)) = rx.recv().await {
            b.set_label(&vol);
            if perc >= 101 {
                b.add_css_class("volume-warning");
            } else {
                b.remove_css_class("volume-warning");
            }
        }
    });

    std::thread::spawn(move || {
        let mut mainloop = Mainloop::new().expect("Failed to create pulse mainloop");
        let mut proplist = pulse::proplist::Proplist::new().unwrap();
        proplist.set_str(pulse::proplist::properties::APPLICATION_NAME, "vibebar-p4-volume").unwrap();
        
        let context = Rc::new(RefCell::new(Context::new_with_proplist(&mainloop, "VolumeContext", &proplist).expect("Failed to create pulse context")));
        
        {
            let mut ctx = context.borrow_mut();
            ctx.connect(None, ContextFlagSet::NOFLAGS, None).expect("Failed to connect context");
        }

        // Wait for context to be ready
        loop {
            let _ = mainloop.iterate(false);
            let state = context.borrow().get_state();
            if state == pulse::context::State::Ready {
                break;
            }
            if !state.is_good() {
                return;
            }
        }

        let tx_cb = tx.clone();
        let context_cb = context.clone();

        let refresh_volume = move || {
            let tx_inner = tx_cb.clone();
            let context_inner = context_cb.clone();
            
            // Get introspector fresh from context borrow
            let introspect = context_inner.borrow().introspect();
            
            introspect.get_server_info(move |server_info| {
                if let Some(default_sink_name) = &server_info.default_sink_name {
                    let sink_name: String = default_sink_name.to_string();
                    let tx_innermost = tx_inner.clone();
                    let context_innermost = context_inner.clone();
                    
                    // Get introspector again fresh for the nested callback
                    context_innermost.borrow().introspect().get_sink_info_by_name(&sink_name, move |sink_res| {
                        if let pulse::callbacks::ListResult::Item(sink_info) = sink_res {
                            let vol = sink_info.volume.avg().0;
                            let perc = (vol as f64 / 65536.0 * 100.0).round() as i32;
                            let muted = sink_info.mute;
                            let icon = if muted { "" } else { "" };
                            let _ = tx_innermost.send((format!("{}  {}%", icon, perc), perc));
                        }
                    });
                }
            });
        };

        // Initial update
        refresh_volume();

        let refresh_volume_cb = Rc::new(refresh_volume);
        let refresh_volume_cb_inner = refresh_volume_cb.clone();

        context.borrow_mut().set_subscribe_callback(Some(Box::new(move |fac, _op, _idx| {
            if fac == Some(Facility::Sink) || fac == Some(Facility::Server) {
                refresh_volume_cb_inner();
            }
        })));

        context.borrow_mut().subscribe(InterestMaskSet::SINK | InterestMaskSet::SERVER, |_| {});

        let _ = mainloop.run();
    });
}
