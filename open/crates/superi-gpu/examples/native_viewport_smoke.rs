//! Presents one real native viewport frame and exits.

use std::sync::Arc;

use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::surface::{NativeViewportSurface, ViewportExtent};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct SmokeApp {
    window: Option<Arc<Window>>,
    viewport: Option<NativeViewportSurface>,
    device: Option<GpuDevice>,
    presented: bool,
}

impl SmokeApp {
    fn create_viewport(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("Superi native viewport smoke")
            .with_inner_size(LogicalSize::new(640, 360))
            .with_visible(true);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create native smoke window"),
        );

        let instance = GpuInstance::new(InstanceOptions::default()).expect("create GPU instance");
        let mut viewport = NativeViewportSurface::create(&instance, Arc::clone(&window))
            .expect("create native wgpu surface");
        let adapter = viewport
            .compatible_adapters(&instance)
            .expect("enumerate surface-compatible adapters")
            .select(&AdapterSelection::default())
            .expect("select a surface-compatible GPU adapter");
        let device =
            pollster::block_on(adapter.create_device(
                &DeviceRequest::default().with_label("superi-native-viewport-smoke"),
            ))
            .expect("create smoke device and queue");

        let size = window.inner_size();
        let extent =
            ViewportExtent::new(size.width.max(1), size.height.max(1), window.scale_factor())
                .expect("validate native viewport extent");
        viewport
            .configure(&device, extent)
            .expect("configure native viewport surface");

        self.window = Some(window);
        self.viewport = Some(viewport);
        self.device = Some(device);
        self.window
            .as_ref()
            .expect("window was stored immediately above")
            .request_redraw();
    }

    fn present_one_frame(&mut self, event_loop: &ActiveEventLoop) {
        if self.presented {
            return;
        }
        self.presented = true;

        let device = self.device.as_ref().expect("smoke device exists");
        let submissions = GpuSubmissionQueue::new(device).expect("claim smoke submission queue");
        let viewport = self.viewport.as_mut().expect("smoke viewport exists");
        let kind = viewport.kind();
        let frame = viewport.acquire_frame(device).expect("acquire smoke frame");
        let view = frame
            .texture()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("superi-native-viewport-smoke-encoder"),
                });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("superi-native-viewport-smoke-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.35,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        let generation = frame.generation();
        let sequence = frame.sequence();
        let fence = frame
            .submit_and_present(
                &submissions,
                Some(encoder.finish()),
                submissions.resources(),
            )
            .expect("submit and present smoke frame");
        submissions
            .wait(&fence)
            .expect("retire presented smoke frame");
        println!(
            "presented native viewport frame: kind={}, generation={generation}, sequence={sequence}",
            kind.code()
        );
        event_loop.exit();
    }
}

impl ApplicationHandler for SmokeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.create_viewport(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => self.present_one_frame(event_loop),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut SmokeApp::default())?;
    Ok(())
}
