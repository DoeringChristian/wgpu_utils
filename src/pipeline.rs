use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::{fs, ops};
use std::fs::File;
use std::io::Read;
use std::str;
use std::sync::Arc;
use crate::Buffer;
use crate::PushConstantLayout;
use crate::PushConstant;

use super::binding;
use std::borrow::Cow;
use anyhow::*;
use wgpu::util::RenderEncoder;
use core::ops::Range;
use core::num::NonZeroU32;
use naga;

const DEFAULT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const DEFAULT_ENTRY_POINT: &'static str = "main";

pub trait RenderData{
    fn pipeline_layout() -> PipelineLayout;
    fn set_bind_groups(&self, render_pass_pipeline: &mut RenderPassPipeline);
}

pub struct FragmentState<'fs>{
    pub targets: Vec<wgpu::ColorTargetState>,
    pub entry_point: &'fs str,
    pub shader: &'fs wgpu::ShaderModule,
}

pub struct FragmentStateBuilder<'fsb>{
    pub targets: Vec<wgpu::ColorTargetState>,
    shader: &'fsb wgpu::ShaderModule,
    entry_point: &'fsb str,
}

impl <'fsb> FragmentStateBuilder<'fsb>{

    pub const DEFAULT_BLEND_STATE: wgpu::BlendState = wgpu::BlendState{
        color: wgpu::BlendComponent::REPLACE,
        alpha: wgpu::BlendComponent::REPLACE,
    };

    pub fn new(shader: &'fsb wgpu::ShaderModule) -> Self{
        Self{
            targets: Vec::new(),
            shader,
            entry_point: DEFAULT_ENTRY_POINT,
        }
    }
    
    pub fn set_entry_point(mut self, entry_point: &'fsb str) -> Self{
        self.entry_point = entry_point;
        self
    }
    
    pub fn push_target(mut self, color_target_state: wgpu::ColorTargetState) -> Self{
        self.targets.push(color_target_state);
        self
    }

    pub fn push_target_replace(mut self, format: wgpu::TextureFormat) -> Self{
        self.targets.push(wgpu::ColorTargetState{
            format,
            blend: Some(wgpu::BlendState{
                color: wgpu::BlendComponent::REPLACE,
                alpha: wgpu::BlendComponent::REPLACE,
            }),
            write_mask: wgpu::ColorWrites::all(),
        });
        self
    }

    pub fn build(&self) -> FragmentState<'fsb>{
        FragmentState{
            targets: self.targets.clone(),
            entry_point: self.entry_point,
            shader: self.shader,
        }
    }

}

///
/// Layout of the VertexState of a Pipeline.
/// It describes the buffer layouts as well as the names used when setting by name in the 
/// RenderPassPipeline process.
///
pub struct VertexState<'vs>{
    pub vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout<'vs>>,
    pub entry_point: &'vs str,
    pub vertex_shader: &'vs wgpu::ShaderModule,
}

pub struct VertexStateBuilder<'vsb>{
    vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout<'vsb>>,
    entry_point: &'vsb str,
    //module: &'vsb wgpu::ShaderModule,
    vertex_shader: &'vsb wgpu::ShaderModule,
    index: usize,
}

impl<'vsb> VertexStateBuilder<'vsb>{
    pub fn new(vertex_shader: &'vsb wgpu::ShaderModule) -> Self{
        Self{
            vertex_buffer_layouts: Vec::new(),
            entry_point: DEFAULT_ENTRY_POINT,
            index: 0,
            vertex_shader,
        }
    }

    pub fn set_entry_point(mut self, entry_point: &'vsb str) -> Self{
        self.entry_point = entry_point;
        self
    }

    pub fn push_vert_layouts(mut self, mut vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout<'vsb>>) -> Self{
        self.vertex_buffer_layouts.append(&mut vertex_buffer_layouts);
        self
    }

    pub fn build(&self) -> VertexState<'vsb>{
        VertexState{
            vertex_buffer_layouts: self.vertex_buffer_layouts.clone(),
            entry_point: self.entry_point,
            vertex_shader: self.vertex_shader,
        }
    }
}

pub struct RenderPipeline{
    pub pipeline: wgpu::RenderPipeline,
    pub push_const_ranges: Vec<wgpu::PushConstantRange>
}

pub struct PipelineLayout{
    pub layout: wgpu::PipelineLayout,
    push_const_ranges: Vec<wgpu::PushConstantRange>,
}

// TODO: put bind_group_names in Arc
pub struct PipelineLayoutBuilder<'l>{
    bind_group_layouts: Vec<&'l binding::BindGroupLayoutWithDesc>,
    push_const_layouts: Vec<PushConstantLayout>,
    index: usize,
}

impl<'l> PipelineLayoutBuilder<'l>{
    pub fn new() -> Self{
        Self{
            bind_group_layouts: Vec::new(),
            push_const_layouts: Vec::new(),
            index: 0,
        }
    }

    pub fn push_bind_group(mut self, bind_group_layout: &'l binding::BindGroupLayoutWithDesc) -> Self{
        self.bind_group_layouts.push(bind_group_layout);
        self
    }

    pub fn push_const_layout(mut self, push_const_layout: PushConstantLayout) -> Self{
        self.push_const_layouts.push(push_const_layout);
        self
    }

    pub fn create(self, device: &wgpu::Device, label: Option<&str>) -> PipelineLayout{

        let mut bind_group_layouts = Vec::with_capacity(self.bind_group_layouts.len());
        for bind_group_layout_desc in self.bind_group_layouts{
            bind_group_layouts.push(&bind_group_layout_desc.layout);
        }

        // Convert the push_const_layouts to push_const_ranges using alignment
        let mut offset = 0;
        let push_const_ranges: Vec<wgpu::PushConstantRange> = self.push_const_layouts.iter()
            .map(|x| {
                // align to 4 bytes.
                let size_aligned = (((x.size as i32 - 4)/4 + 1)*4) as u32;
                let range = Range::<u32>{
                    start: offset,
                    end: offset + size_aligned,
                };
                offset = range.end;
                wgpu::PushConstantRange{
                    stages: x.stages,
                    range,
                }
            }).collect();

        PipelineLayout{
            layout: device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{
                label,
                bind_group_layouts: &bind_group_layouts,
                push_constant_ranges: &push_const_ranges,
            }),
            push_const_ranges,
        }
    }
}

pub struct RenderPassPipeline<'rp, 'rpr>{
    pub render_pass: &'rpr mut RenderPass<'rp>,
    pub pipeline: &'rp RenderPipeline,
    vert_buffer_index: u32,
}

impl<'rp, 'rpr> RenderPassPipeline<'rp, 'rpr>{
    pub fn set_bind_group<B: binding::GetBindGroup>(&mut self, index: u32, bind_group: &'rp B, offsets: &'rp [wgpu::DynamicOffset]){
        self.render_pass.render_pass.set_bind_group(
            index,
            bind_group.get_bind_group(),
            offsets
        );
    }

    pub fn set_push_const<C: PushConstant>(&mut self, index: u32, constant: &C){
        self.render_pass.render_pass.set_push_constants(
            self.pipeline.push_const_ranges[index as usize].stages, 
            self.pipeline.push_const_ranges[index as usize].range.start,
            bytemuck::bytes_of(constant));
    }

    pub fn set_bind_groups(&mut self, bind_groups: &[&'rp wgpu::BindGroup]){
        for (i, bind_group) in bind_groups.iter().enumerate(){
            self.render_pass.render_pass.set_bind_group(
                i as u32,
                bind_group,
                &[],
            )
        }
    }

    pub fn set_vertex_buffer(&mut self, index: u32, buffer_slice: wgpu::BufferSlice<'rp>){
        self.render_pass.render_pass.set_vertex_buffer(
            index,
            buffer_slice
        );
        self.vert_buffer_index = index + 1;
    }

    pub fn push_vertex_buffer(&mut self, buffer_slice: wgpu::BufferSlice<'rp>){
        self.render_pass.render_pass.set_vertex_buffer(
            self.vert_buffer_index,
            buffer_slice
        );
        self.vert_buffer_index += 1;
    }

    pub fn set_index_buffer(&mut self, buffer_slice: wgpu::BufferSlice<'rp>, format: wgpu::IndexFormat){
        self.render_pass.render_pass.set_index_buffer(buffer_slice, format);
    }


    pub fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>){
        self.render_pass.render_pass.draw(vertices, instances);
    }

    pub fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>){
        self.render_pass.render_pass.draw_indexed(indices, base_vertex, instances);
    }

    pub fn set_pipeline(&'rpr mut self, pipeline: &'rp RenderPipeline) -> Self{
        self.render_pass.render_pass.set_pipeline(&pipeline.pipeline);
        Self{
            render_pass: self.render_pass,
            pipeline,
            vert_buffer_index: 0,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DispatchIndirect{
    x: u32,
    y: u32,
    z: u32,
}

///
/// Wrapper for wgpu::ComputePass
///
pub struct ComputePass<'cp>{
    pub cpass: wgpu::ComputePass<'cp>,
}

impl<'cp> ComputePass<'cp>{
    pub fn new(encoder: &'cp mut wgpu::CommandEncoder, label: Option<&str>) -> Self{
        let cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{
            label,
        });
        Self{
            cpass,
        }
    }

    pub fn set_pipeline(&mut self, pipeline: &'cp ComputePipeline) -> ComputePassPipeline<'cp, '_>{
        self.cpass.set_pipeline(&pipeline.pipeline);
        ComputePassPipeline{
            cpass: self,
            pipeline,
        }
    }

}

///
/// A ComputePass with pipeline needed for push_const offsets.
///
pub struct ComputePassPipeline<'cp, 'cpr>{
    pub cpass: &'cpr mut ComputePass<'cp>,
    pub pipeline: &'cp ComputePipeline,
}

impl<'cp, 'cpr> ComputePassPipeline<'cp, 'cpr>{
    pub fn set_bind_group<B: binding::GetBindGroup>(&mut self, index: u32, bind_group: &'cp B, offsets: &'cp [wgpu::DynamicOffset]){
        self.cpass.cpass.set_bind_group(index, bind_group.get_bind_group(), offsets);
    }

    pub fn set_push_const<C: PushConstant>(&mut self, index: u32, constant: &C){
        self.cpass.cpass.set_push_constants(
            self.pipeline.push_const_ranges[index as usize].range.start,
            bytemuck::bytes_of(constant));
    }

    pub fn dispatch(&mut self, x: u32, y: u32, z: u32){
        self.cpass.cpass.dispatch(x, y, z);
    }

    pub fn dispatch_indirect(&mut self, indirect_buffer: &'cp Buffer<DispatchIndirect>, indirect_offset: wgpu::BufferAddress){
        self.cpass.cpass.dispatch_indirect(&indirect_buffer.buffer, indirect_offset);
    }

    pub fn set_pipeline(&'cpr mut self, pipeline: &'cp ComputePipeline) -> Self{
        self.cpass.cpass.set_pipeline(&pipeline.pipeline);
        Self{
            cpass: self.cpass,
            pipeline,
        }
    }
}

///
/// A wrapper for wgpu::ComputePipeline with PushConstantRanges
///
pub struct ComputePipeline{
    pub pipeline: wgpu::ComputePipeline,
    pub push_const_ranges: Vec<wgpu::PushConstantRange>,
}

///
/// A builder for a ComputePipeline
///
///
pub struct ComputePipelineBuilder<'cpb>{
    label: wgpu::Label<'cpb>,
    layout: Option<&'cpb PipelineLayout>,
    module: &'cpb wgpu::ShaderModule,
    entry_point: &'cpb str,
}

impl<'cpb> ComputePipelineBuilder<'cpb>{

    pub fn new(module: &'cpb wgpu::ShaderModule) -> Self{
        Self{
            label: None,
            layout: None,
            module,
            entry_point: "main",
        }
    }

    pub fn set_entry_point(mut self, entry_point: &'cpb str) -> Self{
        self.entry_point = entry_point;
        self
    }

    pub fn set_label(mut self, label: wgpu::Label<'cpb>) -> Self{
        self.label = label;
        self
    }

    pub fn set_layout(mut self, layout: &'cpb PipelineLayout) -> Self{
        self.layout = Some(layout);
        self
    }

    pub fn build(&mut self, device: &wgpu::Device) -> ComputePipeline{
        let layout = self.layout.expect("no layout provided");
        ComputePipeline{
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{
                label: self.label,
                layout: Some(&layout.layout),
                module: self.module,
                entry_point: self.entry_point,
            }),
            push_const_ranges: layout.push_const_ranges.clone(),
        }
    }
}


///
/// A wrapper for wgpu::RenderPass
///
pub struct RenderPass<'rp>{
    pub render_pass: wgpu::RenderPass<'rp>,
}

impl<'rp> RenderPass<'rp>{

    pub fn set_pipeline(&mut self, pipeline: &'rp RenderPipeline) -> RenderPassPipeline<'rp, '_>{
        self.render_pass.set_pipeline(&pipeline.pipeline);
        RenderPassPipeline{
            render_pass: self,
            pipeline,
            vert_buffer_index: 0,
        }
    }

    /* TODO: maybe remove RenderPassPipeline
       #[inline]
       pub fn set_bind_group(&mut self, index: u32, bind_group: &'rp wgpu::BindGroup, offsets: &'rp [wgpu::DynamicOffset]){
       self.render_pass.set_bind_group(index, bind_group, offsets);
       }
       */
}

///
/// A builder for the RenderPass.
///
pub struct RenderPassBuilder<'rp>{
    color_attachments: Vec<wgpu::RenderPassColorAttachment<'rp>>,
}

impl<'rp> RenderPassBuilder<'rp>{
    pub fn new() -> Self{
        Self{
            color_attachments: Vec::new(),
        }
    }

    pub fn push_color_attachment(mut self, color_attachment: wgpu::RenderPassColorAttachment<'rp>) -> Self{
        self.color_attachments.push(color_attachment);
        self
    }

    // TODO: add depth_stencil_attachment
    pub fn begin(self, encoder: &'rp mut wgpu::CommandEncoder, label: Option<&'rp str>) -> RenderPass<'rp>{
        RenderPass{
            render_pass: encoder.begin_render_pass(&wgpu::RenderPassDescriptor{
                label,
                color_attachments: &self.color_attachments,
                depth_stencil_attachment: None,
            }),
        }
    }
}

pub fn shader_load(device: &wgpu::Device, path: &str, stage: naga::ShaderStage, label: Option<&str>) -> Result<wgpu::ShaderModule>{
    let mut f = File::open(path)?;
    let metadata = fs::metadata(path)?;
    let mut buffer = vec![0; metadata.len() as usize];
    f.read(&mut buffer)?;
    let src = str::from_utf8(&buffer)?;


    let extension = Path::new(path).extension().ok_or(anyhow!("No extension"))?;


    let source = match extension.to_str().ok_or(anyhow!("string conversion"))?{
        "glsl" => wgpu::ShaderSource::Glsl{
            shader: Cow::from(src),
            stage,
            defines: naga::FastHashMap::default()
        },
        "wgsl" => wgpu::ShaderSource::Wgsl(Cow::from(src)),
        _ => return Err(anyhow!("Unknown Extension")),
    };

    Ok(device.create_shader_module(&wgpu::ShaderModuleDescriptor{
        label,
        source,
    }))
}

pub fn shader_with_shaderc(device: &wgpu::Device, src: &str, kind: shaderc::ShaderKind, entry_point: &str, label: Option<&str>) -> Result<wgpu::ShaderModule>{

    let mut compiler = shaderc::Compiler::new().ok_or(anyhow!("error creating compiler"))?;
    let mut options = shaderc::CompileOptions::new().ok_or(anyhow!("error creating shaderc options"))?;

    options.set_warnings_as_errors();
    options.set_target_env(shaderc::TargetEnv::Vulkan, 0);
    options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    options.set_generate_debug_info();

    options.add_macro_definition("VERTEX_SHADER", Some(if kind == shaderc::ShaderKind::Vertex {"1"} else {"0"}));
    options.add_macro_definition("FRAGMENT_SHADER", Some(if kind == shaderc::ShaderKind::Fragment {"1"} else {"0"}));
    options.add_macro_definition("COMPUTE_SHADER", Some(if kind == shaderc::ShaderKind::Compute {"1"} else {"0"}));

    /*
       options.set_include_callback(|name, include_type, source_file, _depth|{
       let path = if include_type == shaderc::IncludeType::Relative{
       Path::new(Path::new(source_file).parent().unwrap()).join(name)
       }
       });
       */

    //println!("{:?}: \n{}", label, compiler.preprocess(src, "preprocess", entry_point, Some(&options)).unwrap().as_text());

    let spirv = match label{
        Some(label) => compiler.compile_into_spirv(src, kind, label, entry_point, Some(&options))?,
        _ => compiler.compile_into_spirv(src, kind, "no_label", entry_point, Some(&options))?,
    };

    let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor{
        label,
        source: wgpu::ShaderSource::SpirV(Cow::from(spirv.as_binary()))
    });

    Ok(module)
}

pub struct RenderPipelineBuilder<'rpb>{
    label: Option<&'rpb str>,
    layout: Option<&'rpb PipelineLayout>,
    vertex: VertexState<'rpb>,
    fragment: FragmentState<'rpb>,
    primitive: wgpu::PrimitiveState,
    depth_stencil: Option<wgpu::DepthStencilState>,
    multisample: wgpu::MultisampleState,
    multiview: Option<NonZeroU32>,
}

impl<'rpb> RenderPipelineBuilder<'rpb>{

    pub fn new(vertex: VertexState<'rpb>, fragment: FragmentState<'rpb>) -> Self{
        let label = None;
        let layout = None;
        let primitive = wgpu::PrimitiveState{
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        };
        let depth_stencil = None;
        let multisample = wgpu::MultisampleState{
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let multiview = None;
        Self{
            label,
            layout,
            vertex,
            fragment,
            primitive,
            depth_stencil,
            multisample,
            multiview,
        }
    }

    pub fn set_layout(mut self, layout: &'rpb PipelineLayout) -> Self{
        self.layout = Some(layout);
        self
    }

    pub fn set_label(mut self, label: wgpu::Label<'rpb>) -> Self{
        self.label = label;
        self
    }

    pub fn build(self, device: &wgpu::Device) -> RenderPipeline{

        /*
           let layout = match self.layout{
           Some(l) => Some(&l.layout),
           _ => None,
           };
           */
        let layout = self.layout.expect("no layout provided");
        /*
           let fragment = match self.fragment{
           Some(f) => Some(wgpu::FragmentState{
           module: f.shader,
           entry_point: f.entry_point,
           targets: &self.fragment.unwrap().color_target_states,
           }),
           _ => None,
           };
           */
        let fragment = wgpu::FragmentState{
            module: self.fragment.shader,
            entry_point: self.fragment.entry_point,
            targets: &self.fragment.targets,
        };

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
            label: self.label,
            layout: Some(&layout.layout),
            vertex: wgpu::VertexState{
                module: self.vertex.vertex_shader,
                entry_point: self.vertex.entry_point,
                buffers: &self.vertex.vertex_buffer_layouts,
            },
            fragment: Some(fragment),
            primitive: self.primitive,
            depth_stencil: self.depth_stencil,
            multisample: self.multisample,
            multiview: self.multiview,
        });

        RenderPipeline{
            pipeline: render_pipeline,
            push_const_ranges: layout.push_const_ranges.clone(),
        }
    }
}




// TODO:
// Counting RenderPass
