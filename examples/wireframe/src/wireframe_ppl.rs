use ewgpu::*;

use crate::wireframe::{WireframeMeshVert, WireframeVert};

use super::camera::Camera;

#[derive(DerefMut)]
pub struct WireframeRenderPipeline {
    rppl: wgpu::RenderPipeline,
}

impl PipelineLayout for WireframeRenderPipeline {}

impl WireframeRenderPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let vshader =
            VertexShader::from_src_glsl(device, include_str!("shaders/wf_mesh_rppl.glsl"), None)
                .unwrap();
        let fshader =
            FragmentShader::from_src_glsl(device, include_str!("shaders/wf_mesh_rppl.glsl"), None)
                .unwrap();

        let rppl = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &vshader,
                entry_point: "main",
                buffers: &[WireframeMeshVert::buffer_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fshader,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::all(),
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multiview: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        Self { rppl }
    }
}

#[derive(DerefMut)]
pub struct WireframeMeshPipeline {
    cppl: wgpu::ComputePipeline,
}

impl WireframeMeshPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader =
            ComputeShader::from_src_glsl(device, include_str!("shaders/line_ppl.glsl"), None)
                .unwrap();

        let cppl = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&Self::layout(device).unwrap()),
            module: &shader,
            entry_point: "main",
        });

        Self { cppl }
    }
}

impl PipelineLayout for WireframeMeshPipeline {
    fn layout(device: &wgpu::Device) -> Option<wgpu::PipelineLayout> {
        Some(
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::all(),
                                ty: wgsl::buffer(false),
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::all(),
                                ty: wgsl::buffer(false),
                                count: None,
                            },
                        ],
                    }),
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::all(),
                                ty: wgsl::buffer(false),
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::all(),
                                ty: wgsl::buffer(false),
                                count: None,
                            },
                        ],
                    }),
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::all(),
                            ty: wgsl::uniform(),
                            count: None,
                        }],
                    }),
                ],
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::COMPUTE,
                    range: 0..(std::mem::size_of::<Camera>() as u32),
                }],
            }),
        )
    }
}
