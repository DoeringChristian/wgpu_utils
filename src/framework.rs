use imgui_winit_support::WinitPlatform;
#[allow(unused)]
use winit::{
    event::*,
    event_loop::{
        ControlFlow,
        EventLoop,
    },
    window::{
        Window,
        WindowBuilder,
    },
};

use std::{time::{Instant, Duration}, ops::{Deref, DerefMut}};
use super::*;

#[allow(unused)]
pub trait ImguiState{
    fn new(gpu: &mut WinitContext, imgui: &mut ImguiContext) -> Self;
    fn render(&mut self, gpu: &mut WinitContext, imgui: &mut ImguiContext, dst: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder, control_flow: &mut ControlFlow) -> Result<(), wgpu::SurfaceError>{Ok(())}
    fn event(&mut self, gpu: &mut WinitContext, imgui: &mut ImguiContext, event: &WindowEvent) -> bool{false}
    fn resize(&mut self, gpu: &mut WinitContext, imgui: &mut ImguiContext, new_size: winit::dpi::PhysicalSize<u32>){}
}

#[allow(unused)]
pub trait State{
    fn new(gpu: &mut GPUContext) -> Self;
    fn render(&mut self, gpu: &mut GPUContext);
}

pub struct ImguiContext{
    pub context: imgui::Context,
    pub platform: WinitPlatform,
    pub renderer: imgui_wgpu::Renderer,
}

pub struct ImguiInternal{
    pub context: imgui::Context,
    pub platform: WinitPlatform,
    pub renderer: imgui_wgpu::Renderer,
}

impl ImguiContext{
    pub fn new(winit_context: &WinitContext) -> Self{

        let mut context = imgui::Context::create();
        let mut platform = imgui_winit_support::WinitPlatform::init(&mut context);
        platform.attach_window(
            context.io_mut(),
            &winit_context.window,
            imgui_winit_support::HiDpiMode::Default,
        );
        context.set_ini_filename(None);

        let hidpi_factor = winit_context.window.scale_factor();
        let font_size = (13.0 * hidpi_factor) as f32;
        context.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        context.fonts().add_font(&[
            imgui::FontSource::DefaultFontData{
                config: Some(imgui::FontConfig{
                    oversample_h: 1,
                    pixel_snap_h: true,
                    size_pixels: font_size,
                    ..Default::default()
                }),
            }
        ]);

        let renderer_config = imgui_wgpu::RendererConfig{
            texture_format: winit_context.config.format,
            ..Default::default()
        };

        let renderer = imgui_wgpu::Renderer::new(&mut context, &winit_context.device, &winit_context.queue, renderer_config);

        Self{
            renderer,
            context,
            platform,
        }
    }

    pub fn ui<F>(&mut self, winit_context: &mut WinitContext, encoder: &mut wgpu::CommandEncoder, dst: &wgpu::TextureView, mut f: F)
        where F: FnMut(ImguiRenderContext, &mut WinitContext, &mut wgpu::CommandEncoder)
    {
        self.platform
            .prepare_frame(self.context.io_mut(), &winit_context.window)
            .expect("Failed to prepare frame.");

        self.context.io_mut().update_delta_time(winit_context.dt);

        let ui = self.context.frame();

        let imgui_render_context = ImguiRenderContext{
            platform: &self.platform,
            renderer: &self.renderer,
            ui: &ui,
        };

        f(imgui_render_context, winit_context, encoder);

        let mut rpass = RenderPassBuilder::new()
            .push_color_attachment(dst.color_attachment_load())
            .begin(encoder, None);

        self.renderer.render(ui.render(), &winit_context.queue, &winit_context.device, &mut rpass.render_pass)
            .expect("Rendering Failed");

        drop(rpass);
    }
}


pub struct ImguiRenderContext<'irc, 'ui>{
    pub platform: &'irc WinitPlatform,
    pub renderer: &'irc imgui_wgpu::Renderer,
    pub ui: &'irc imgui::Ui<'ui>,
}

impl<'irc, 'ui> Deref for ImguiRenderContext<'irc, 'ui>{
    type Target = imgui::Ui<'ui>;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

pub struct Framework<S>{
    pub instance: wgpu::Instance,
    pub gpu: GPUContext,
    pub state: S,
}

impl<S> Framework<S>{
    pub fn new<F>(f: F) -> Self
        where F: Fn(&mut GPUContext) -> S
    {
        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let mut gpu = GPUContext::new(&instance, None);

        let state = f(&mut gpu);

        Self{
            instance,
            gpu,
            state,
        }
    }
    pub fn run<F>(mut self, mut f: F)
        where F: FnMut(&mut S, &mut GPUContext)
    {
        f(&mut self.state, &mut self.gpu);
    }
}

impl<S: State> Framework<S>{
    pub fn new_state() -> Self{
        Self::new(S::new)
    }
    pub fn run_state(self){
        self.run(S::render)
    }
}

pub struct ImguiFramework<S>
{
    pub imgui: ImguiContext,
    pub winit: WinitContext,
    pub instance: wgpu::Instance,
    pub state: S,
    pub event_loop: EventLoop<()>,

}
impl<S: 'static> ImguiFramework<S>
{
    pub fn new<F>(window_builder: WindowBuilder, new: F) -> Self
        where F: Fn(&mut WinitContext, &mut ImguiContext) -> S
    {
        env_logger::init();
        let event_loop = EventLoop::new();

        let window = window_builder.build(&event_loop).unwrap();

        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let mut winit = WinitContext::new(&instance, window);
        let mut imgui = ImguiContext::new(&winit);

        let state = new(&mut winit, &mut imgui);

        Self{
            instance,
            imgui,
            winit,
            event_loop,
            state,
        }
    }
    pub fn run<R: 'static, E: 'static, RZ: 'static>(mut self, mut render_cb: R, mut event_cb: E, mut resize: RZ)
        where R: FnMut(&mut S, &mut WinitContext, &mut ImguiContext, &wgpu::TextureView, &mut wgpu::CommandEncoder, &mut ControlFlow) -> Result<(), wgpu::SurfaceError>,
              E: FnMut(&mut S, &mut WinitContext, &mut ImguiContext, &WindowEvent) -> bool,
              RZ: FnMut(&mut S, &mut WinitContext, &mut ImguiContext, winit::dpi::PhysicalSize<u32>),
    {
        self.event_loop.run(move |event, _, control_flow|{
            self.imgui.platform.handle_event(self.imgui.context.io_mut(), &self.winit.window, &event);
            match event{
                Event::WindowEvent{
                    ref event,
                    window_id,
                } if window_id == self.winit.window.id() => if !event_cb(&mut self.state, &mut self.winit, &mut self.imgui, &event){
                    match event{
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::Resized(physical_size) => {
                            self.winit.resize(*physical_size);
                            resize(&mut self.state, &mut self.winit, &mut self.imgui, *physical_size);
                        },
                        WindowEvent::ScaleFactorChanged{new_inner_size, ..} => {
                            self.winit.resize(**new_inner_size);
                            resize(&mut self.state, &mut self.winit, &mut self.imgui, **new_inner_size);
                        },
                        _ => {},
                    }
                },

                Event::RedrawRequested(window_id) if window_id == self.winit.window.id() => {
                    let output = match self.winit.surface.get_current_texture(){
                        Ok(o) => {o},
                        Err(e) => {eprintln!("{:?}", e); return},
                    };
                    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

                    let mut encoder = self.winit.device.create_command_encoder(&wgpu::CommandEncoderDescriptor{label: Some("imgui_encoder")});

                    // Call render function 

                    match render_cb(&mut self.state, &mut self.winit, &mut self.imgui, &view, &mut encoder, control_flow){
                        Ok(_) => {}

                        Err(wgpu::SurfaceError::Lost) => self.winit.resize(self.winit.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,

                        Err(e) => eprintln!("{:?}", e),
                    }

                    self.winit.queue.submit(Some(encoder.finish()));
                    output.present();
                    self.winit.update();
                },

                Event::MainEventsCleared => {
                    self.winit.window.request_redraw();
                },
                _ => {}
            }
            //self.imgui.platform.handle_event(self.imgui.context.io_mut(), &self.winit.window, &event);
        });
    }
}

impl<S: 'static + ImguiState> ImguiFramework<S>{
    pub fn new_state(window_builder: WindowBuilder) -> Self{
        Self::new(window_builder, |gpu, imgui|{
            S::new(gpu, imgui)
        })
    }

    pub fn run_state(self){
        self.run(S::render, S::event, S::resize)
    }
}
