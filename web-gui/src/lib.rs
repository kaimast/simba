#![allow(clippy::arc_with_non_send_sync)]

use simba_visualizer::graphics::{Graphics, RenderLoop};
use simba_visualizer::scene::SceneManager;
use simba_visualizer::ui::{CursorPosition, UiEvents, UiMessages};
use simba_visualizer::window_loop::WindowLoop;

use simba::{Failures, NetworkConfiguration, ProtocolConfiguration, Simulation};

use std::process::exit;
use std::sync::Arc;

use anyhow::Context;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use winit::platform::web::WindowBuilderExtWebSys;
use winit::window::WindowBuilder;

use winit::event_loop::EventLoop as WinitEventLoop;

#[wasm_bindgen]
pub fn run_simulation() -> Result<(), JsValue> {
    console_log::init_with_level(log::Level::Debug).unwrap();
    log::info!("Starting SimBA Web UI");

    let tokio_rt = tokio::runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to start prokio");

    tokio_rt.block_on(async move {
        async_run()
            .await
            .map_err(|err| JsValue::from(format!("Fatal error: {err}")))
    })
}

async fn async_run() -> anyhow::Result<()> {
    let ui_messages = Arc::new(UiMessages::default());
    let ui_events = Arc::new(UiEvents::default());

    let winit_loop = WinitEventLoop::new().expect("Failed to initialize winit");

    let dom_window = web_sys::window().expect("Can not get browser window.");
    let document = dom_window.document().expect("Can not get html document.");

    let cursor_position = Arc::new(CursorPosition::default());

    let canvas = document
        .get_element_by_id("render_canvas")
        .expect("The given canvas id was not found in the document.");

    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| {
            log::error!("Failed to convert element to canvas");
            exit(-1);
        })
        .unwrap();

    let window = WindowBuilder::new()
        .with_title("SimBA")
        .with_canvas(Some(canvas))
        .with_resizable(false)
        .build(&winit_loop)
        .with_context(|| "Failed to create web window")?;

    log::info!("Started with window size: {:?}", window.inner_size());

    log::debug!("Setting up graphics");
    let (graphics, surface) = Graphics::new(&window)
        .await
        .with_context(|| "Failed to set up graphics")?;
    let graphics = Arc::new(graphics);

    log::debug!("Setting up simulation");
    let network = NetworkConfiguration::default();
    let protocol = ProtocolConfiguration::default();
    let failures = Failures::new(network.num_nodes(), None);

    let simulation = Arc::new(Simulation::new(protocol, network, failures).unwrap());

    log::debug!("Setting up scene manager");
    let scene_mgr = Arc::new(
        SceneManager::new(graphics.clone(), ui_messages.clone(), simulation.clone()).await,
    );

    log::debug!("Everything set up!");

    // Set simulation speed to 1000x of real time
    simulation.set_rate_limit(1000);

    // Start simulation in the background
    simulation.start();

    let task = {
        let graphics = graphics.clone();
        let ui_events = ui_events.clone();
        let cursor_position = cursor_position.clone();
        let simulation = simulation.clone();
        let scene_mgr = scene_mgr.clone();

        // Window is not send on WebAssembly
        let mut render_loop = RenderLoop::new(
            //tokio_rt_hdl.clone(),
            graphics,
            ui_messages,
            ui_events,
            cursor_position,
            window,
            surface,
            simulation,
            scene_mgr.clone(),
        )
        .await;

        simba_visualizer::spawn_task(async move {
            render_loop.run().await;
        })
    };

    let window_loop = WindowLoop::default();
    window_loop
        .run(winit_loop, ui_events, graphics, scene_mgr, cursor_position)
        .unwrap();

    task.abort();
    simulation.stop();

    Ok(())
}
