// Clippy bug
#![allow(clippy::needless_return)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;

use winit::dpi::{LogicalSize, Size};
use winit::event_loop::EventLoop as WinitEventLoop;
use winit::window::WindowAttributes;

use clap::Parser;

use simba_visualizer::graphics::{Graphics, RenderLoop};
use simba_visualizer::scene::SceneManager;
use simba_visualizer::ui::{CursorPosition, UiEvents, UiMessages};
use simba_visualizer::window_loop::WindowLoop;

use simba::{Failures, Library, Simulation};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(help = "The name of the network to run on")]
    network_name: String,

    #[clap(help = "The name of the protocol to run")]
    protocol_name: String,

    #[clap(long, short = 'p', default_value = "./library")]
    #[clap(help = "Where to look for the configuration files?")]
    library_path: String,

    #[clap(long, short = 'j', required = false)]
    #[clap(help = "How many concurrent tasks? Will be the number of cores by default")]
    parallelism: Option<usize>,

    #[clap(long)]
    enable_tokio_console: bool,

    #[clap(long)]
    #[clap(help = "Pause the simulation on startup")]
    start_paused: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // TODO support logging and tracing at the same time
    if args.enable_tokio_console {
        console_subscriber::init();
    } else {
        env_logger::init();
    }

    let library = match Library::new(args.library_path) {
        Ok(library) => library,
        Err(err) => {
            log::error!("Failed to open library: {err}");
            std::process::exit(-1);
        }
    };

    let protocol = library.get_protocol(&args.protocol_name)?.clone();
    let network = library.get_network(&args.network_name)?.clone();

    let ui_messages = Arc::new(UiMessages::default());
    let ui_events = Arc::new(UiEvents::default());

    let winit_loop = WinitEventLoop::new().with_context(|| "Create winit event loop")?;

    let cursor_position = Arc::new(CursorPosition::default());

    let attributes = WindowAttributes::default()
        .with_title("SimBA")
        .with_resizable(true)
        .with_inner_size(Size::Logical(LogicalSize::new(1440.0, 900.0)));

    #[allow(deprecated)]
    let window = winit_loop
        .create_window(attributes)
        .with_context(|| "Create window")?;

    log::info!("Started with window size: {:?}", window.inner_size());

    let (graphics, surface) = Graphics::new(&window).await?;
    let graphics = Arc::new(graphics);
    let failures = Failures::new(network.num_nodes(), None);

    let simulation = Arc::new(
        Simulation::new(protocol, network, failures, None)
            .with_context(|| "Failed to create simulation")?,
    );

    let scene_mgr = Arc::new(
        SceneManager::new(graphics.clone(), ui_messages.clone(), simulation.clone()).await,
    );

    log::debug!("Everything set up!");

    if args.start_paused {
        simulation.set_rate_limit(0);
    } else {
        // Start simulation speed to 10x of real time
        simulation.set_rate_limit(1_000);
    }

    // Start simulation in the background
    simulation.start();

    log::debug!("Starting render loop");

    let stop_flag = Arc::new(AtomicBool::new(false));

    let render_thread = {
        let graphics = graphics.clone();
        let simulation = simulation.clone();
        let scene_mgr = scene_mgr.clone();
        let ui_events = ui_events.clone();
        let cursor_position = cursor_position.clone();
        let stop_flag = stop_flag.clone();

        std::thread::spawn(move || {
            let tokio_rt =
                tokio::runtime::LocalRuntime::new().expect("Failed to create local runtime");

            tokio_rt.block_on(async move {
                let mut render_loop = RenderLoop::new(
                    graphics,
                    ui_messages,
                    ui_events,
                    cursor_position,
                    window,
                    surface,
                    simulation,
                    scene_mgr,
                    stop_flag,
                )
                .await;

                render_loop.run().await;
            })
        })
    };

    let window_loop = WindowLoop::default();
    window_loop.run(winit_loop, ui_events, graphics, scene_mgr, cursor_position)?;

    stop_flag.store(true, Ordering::SeqCst);

    let _ = render_thread.join();
    simulation.stop();

    Ok(())
}
