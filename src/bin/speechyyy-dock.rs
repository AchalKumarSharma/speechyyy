use smithay_client_toolkit::compositor::{
    CompositorHandler, CompositorState, FrameCallbackData, Region,
};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::reexports::calloop::{channel, EventLoop};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_shm, wl_surface};
use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry};
use speechyyy::dock::{DockModel, DockRenderer};
use speechyyy::events::DaemonEvent;
use speechyyy::ipc::RuntimePaths;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DOCK_WIDTH: u32 = 280;
const DOCK_HEIGHT: u32 = 48;

struct DockApp {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    renderer: DockRenderer,
    model: DockModel,
    last_frame_time: Instant,
    frame_pending: bool,
    first_configure: bool,
    exit: bool,
}

impl CompositorHandler for DockApp {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.frame_pending = false;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        self.model.tick(dt);
        self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for DockApp {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for DockApp {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

impl ShmHandler for DockApp {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl DockApp {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let (buffer, canvas) = match self.pool.create_buffer(
            DOCK_WIDTH as i32,
            DOCK_HEIGHT as i32,
            (DOCK_WIDTH * 4) as i32,
            wl_shm::Format::Argb8888,
        ) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("speechyyy-dock: failed to allocate shm buffer: {}", e);
                return;
            }
        };

        // Render dock pill and visuals onto the shared memory canvas
        self.renderer.render(&self.model, canvas);

        // Mark surface damaged and attach buffer
        self.layer
            .wl_surface()
            .damage_buffer(0, 0, DOCK_WIDTH as i32, DOCK_HEIGHT as i32);

        if let Err(e) = buffer.attach_to(self.layer.wl_surface()) {
            eprintln!("speechyyy-dock: buffer attach error: {}", e);
            return;
        }

        // If active or animating, request next VSync frame callback
        if self.model.is_animating() {
            self.layer
                .wl_surface()
                .frame(qh, FrameCallbackData(self.layer.wl_surface().clone()));
            self.frame_pending = true;
        } else {
            self.frame_pending = false;
        }

        self.layer.commit();
    }
}

delegate_registry!(DockApp);

impl ProvidesRegistryState for DockApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_dispatch2!(DockApp);

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&r));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&r));

    // Connect to Wayland server
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "speechyyy-dock: Wayland display not detected (WAYLAND_DISPLAY unset or unreachable: {}). Exiting.",
                e
            );
            std::process::exit(1);
        }
    };

    let (globals, event_queue) = match registry_queue_init(&conn) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("speechyyy-dock: failed to initialize Wayland registry: {}", e);
            std::process::exit(1);
        }
    };

    let qh = event_queue.handle();

    // Bind protocols
    let compositor = match CompositorState::bind(&globals, &qh) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("speechyyy-dock: wl_compositor unavailable: {}", e);
            std::process::exit(1);
        }
    };

    let layer_shell = match LayerShell::bind(&globals, &qh) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("speechyyy-dock: zwlr_layer_shell_v1 unavailable on compositor: {}", e);
            std::process::exit(1);
        }
    };

    let shm = match Shm::bind(&globals, &qh) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("speechyyy-dock: wl_shm unavailable: {}", e);
            std::process::exit(1);
        }
    };

    // Create Layer Surface
    let surface = compositor.create_surface(&qh);

    // Empty input region makes dock 100% click-through
    let empty_region = Region::new(&compositor).ok();
    if let Some(ref reg) = empty_region {
        surface.set_input_region(Some(reg.wl_region()));
    }

    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("speechyyy-dock"),
        None,
    );

    // Anchoring: bottom-center, 36px bottom margin, zero exclusive zone, zero keyboard focus
    layer.set_anchor(Anchor::BOTTOM);
    layer.set_margin(0, 0, 36, 0);
    layer.set_exclusive_zone(0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(DOCK_WIDTH, DOCK_HEIGHT);
    layer.commit();

    let pool = match SlotPool::new((DOCK_WIDTH * DOCK_HEIGHT * 4 * 2) as usize, &shm) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("speechyyy-dock: failed to create shared memory pool: {}", e);
            std::process::exit(1);
        }
    };

    let mut event_loop: EventLoop<DockApp> = match EventLoop::try_new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("speechyyy-dock: failed to initialize event loop: {}", e);
            std::process::exit(1);
        }
    };

    let handle = event_loop.handle();

    // Insert Wayland source into calloop
    if let Err(e) = WaylandSource::new(conn, event_queue).insert(handle.clone()) {
        eprintln!("speechyyy-dock: failed to insert Wayland source into event loop: {}", e);
        std::process::exit(1);
    }

    // Channel for IPC daemon events
    let (tx, channel) = channel::channel::<DaemonEvent>();
    let qh_for_events = qh.clone();

    if let Err(e) = handle.insert_source(channel, move |event, _, app: &mut DockApp| {
        if let channel::Event::Msg(daemon_event) = event {
            app.model.handle_event(daemon_event);

            // If dock has started animating and no frame is currently pending, trigger draw immediately
            if app.model.is_animating() && !app.frame_pending {
                app.last_frame_time = Instant::now();
                app.draw(&qh_for_events);
            }
        }
    }) {
        eprintln!("speechyyy-dock: failed to insert IPC channel into event loop: {}", e);
        std::process::exit(1);
    }

    // Spawn IPC background thread connecting to $XDG_RUNTIME_DIR/speechyyy/events.sock
    let socket_path = RuntimePaths::default().events_socket_path;
    let thread_running = Arc::clone(&running);

    std::thread::spawn(move || {
        while thread_running.load(Ordering::Relaxed) {
            match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    let reader = BufReader::new(stream);
                    for line in reader.lines() {
                        if !thread_running.load(Ordering::Relaxed) {
                            return;
                        }
                        match line {
                            Ok(l) => {
                                if let Ok(event) = serde_json::from_str::<DaemonEvent>(&l) {
                                    if tx.send(event).is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(_) => break, // Reconnect on disconnect
                        }
                    }
                }
                Err(_) => {
                    // Daemon not running yet, sleep backoff
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });

    let mut app = DockApp {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer,
        renderer: DockRenderer::new(DOCK_WIDTH, DOCK_HEIGHT),
        model: DockModel::new(),
        last_frame_time: Instant::now(),
        frame_pending: false,
        first_configure: true,
        exit: false,
    };

    println!("speechyyy-dock: running in background, listening on events.sock");

    // Event loop dispatch
    while running.load(Ordering::Relaxed) && !app.exit {
        // Dispatch with a 50ms timeout so signals and state timers are promptly checked
        if let Err(e) = event_loop.dispatch(Some(Duration::from_millis(50)), &mut app) {
            eprintln!("speechyyy-dock: event loop error: {}", e);
            break;
        }
    }

    println!("speechyyy-dock: shutting down cleanly");
}
