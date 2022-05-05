use crate::*;
use anyhow::*;
use ewgpu_macros::DerefMut;
use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str;

///
/// Wraper for wgpu::ShaderModule using shaderc to load shader modules.
///
/// TODO: Automatic reload if files update.
/// Inspiration from Wumpf's [blub](https://github.com/Wumpf/blub/blob/master/src/wgpu_utils/shader.rs).
///
#[derive(Debug, DerefMut)]
pub struct ShaderModule {
    #[target]
    pub module: wgpu::ShaderModule,
    pub src_files: Vec<PathBuf>,
    pub entry_point: String,
}

impl ShaderModule {
    pub fn from_src_wgls(
        device: &wgpu::Device,
        src: &str,
        entry_point: &str,
        label: wgpu::Label,
    ) -> Result<Self> {
        let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(src)),
        });
        let src_files = Vec::new();
        let entry_point = src.into();

        Ok(ShaderModule {
            module,
            src_files,
            entry_point,
        })
    }
    pub fn from_src_glsl(
        device: &wgpu::Device,
        src: &str,
        stage: wgpu::ShaderStages,
        entry_point: &str,
        label: Option<&str>,
    ) -> Result<Self> {
        let mut compiler = shaderc::Compiler::new().ok_or(anyhow!("error creating compiler"))?;
        let mut options =
            shaderc::CompileOptions::new().ok_or(anyhow!("error creating shaderc options"))?;

        options.set_warnings_as_errors();
        options.set_target_env(shaderc::TargetEnv::Vulkan, 0);
        options.set_optimization_level(shaderc::OptimizationLevel::Performance);
        options.set_generate_debug_info();

        options.add_macro_definition(
            "VERTEX_SHADER",
            Some(if stage == wgpu::ShaderStages::VERTEX {
                "1"
            } else {
                "0"
            }),
        );
        options.add_macro_definition(
            "FRAGMENT_SHADER",
            Some(if stage == wgpu::ShaderStages::FRAGMENT {
                "1"
            } else {
                "0"
            }),
        );
        options.add_macro_definition(
            "COMPUTE_SHADER",
            Some(if stage == wgpu::ShaderStages::COMPUTE {
                "1"
            } else {
                "0"
            }),
        );

        let kind = match stage {
            wgpu::ShaderStages::VERTEX => shaderc::ShaderKind::Vertex,
            wgpu::ShaderStages::FRAGMENT => shaderc::ShaderKind::Fragment,
            wgpu::ShaderStages::COMPUTE => shaderc::ShaderKind::Compute,
            _ => return Err(anyhow!("Shader stage not supported")),
        };

        let spirv = match label {
            Some(label) => {
                compiler.compile_into_spirv(src, kind, label, entry_point, Some(&options))?
            }
            _ => compiler.compile_into_spirv(src, kind, "no_label", entry_point, Some(&options))?,
        };

        let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label,
            source: wgpu::ShaderSource::SpirV(Cow::from(spirv.as_binary())),
        });

        Ok(ShaderModule {
            module,
            src_files: Vec::new(),
            entry_point: String::from(entry_point),
        })
    }

    pub fn load_glsl(
        device: &wgpu::Device,
        path: &Path,
        stage: wgpu::ShaderStages,
        entry_point: &str,
        label: Option<&str>,
    ) -> Result<Self> {
        let src_files = RefCell::new(vec![PathBuf::from(path).canonicalize().unwrap()]);

        let module = {
            let path = src_files.borrow();
            let dir = path[0].parent().unwrap();
            let src = match std::fs::read_to_string(&src_files.borrow()[0]) {
                std::result::Result::Ok(src) => src,
                Err(err) => {
                    return Err(anyhow!(
                        "Failed to read shader file \"{:?}\": {}",
                        &src_files.borrow()[0],
                        err
                    ));
                }
            };

            let mut compiler =
                shaderc::Compiler::new().ok_or(anyhow!("error creating compiler"))?;
            let mut options =
                shaderc::CompileOptions::new().ok_or(anyhow!("error creating shaderc options"))?;

            options.set_warnings_as_errors();
            options.set_target_env(shaderc::TargetEnv::Vulkan, 0);
            options.set_optimization_level(shaderc::OptimizationLevel::Performance);
            options.set_generate_debug_info();

            options.add_macro_definition(
                "VERTEX_SHADER",
                Some(if stage == wgpu::ShaderStages::VERTEX {
                    "1"
                } else {
                    "0"
                }),
            );
            options.add_macro_definition(
                "FRAGMENT_SHADER",
                Some(if stage == wgpu::ShaderStages::FRAGMENT {
                    "1"
                } else {
                    "0"
                }),
            );
            options.add_macro_definition(
                "COMPUTE_SHADER",
                Some(if stage == wgpu::ShaderStages::COMPUTE {
                    "1"
                } else {
                    "0"
                }),
            );

            let kind = match stage {
                wgpu::ShaderStages::VERTEX => shaderc::ShaderKind::Vertex,
                wgpu::ShaderStages::FRAGMENT => shaderc::ShaderKind::Fragment,
                wgpu::ShaderStages::COMPUTE => shaderc::ShaderKind::Compute,
                _ => return Err(anyhow!("Shader stage not supported")),
            };

            options.set_include_callback(|name, include_type, source_file, _depth| {
                let path = if include_type == shaderc::IncludeType::Relative {
                    Path::new(Path::new(source_file).parent().unwrap()).join(name)
                } else {
                    dir.join(name)
                };

                match std::fs::read_to_string(&path) {
                    std::result::Result::Ok(glsl_code) => {
                        src_files.borrow_mut().push(path.canonicalize().unwrap());
                        std::result::Result::Ok(shaderc::ResolvedInclude {
                            resolved_name: String::from(name),
                            content: glsl_code,
                        })
                    }
                    Err(err) => std::result::Result::Err(format!(
                        "Failed to resolve include to {} in {} (was looking for {:?}): {}",
                        name, source_file, path, err
                    )),
                }
            });

            let spirv = compiler.compile_into_spirv(
                &src,
                kind,
                src_files.borrow()[0]
                    .to_str()
                    .ok_or("Path could not be converted to string")
                    .unwrap(),
                entry_point,
                Some(&options),
            )?;

            let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label,
                source: wgpu::ShaderSource::SpirV(Cow::from(spirv.as_binary())),
            });
            module
        };
        Ok(ShaderModule {
            module,
            src_files: src_files.into_inner(),
            entry_point: String::from(entry_point),
        })
    }

    pub fn entry_point(&self) -> &str {
        &self.entry_point
    }
}

#[derive(Debug, DerefMut)]
pub struct FragmentShader {
    module: ShaderModule,
}

impl FragmentShader {
    pub fn from_src_glsl(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::from_src_glsl(
                device,
                src,
                wgpu::ShaderStages::FRAGMENT,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }

    pub fn from_src_wgls(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(FragmentShader {
            module: ShaderModule::from_src_wgls(device, src, DEFAULT_ENTRY_POINT, label)?,
        })
    }

    pub fn load_glsl(device: &wgpu::Device, path: &Path, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::load_glsl(
                device,
                path,
                wgpu::ShaderStages::FRAGMENT,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }

    pub fn fragment_state(&self) -> wgpu::FragmentState {
        wgpu::FragmentState {
            module: self,
            entry_point: self.entry_point(),
            targets: &[],
        }
    }
}

#[derive(Debug, DerefMut)]
pub struct VertexShader {
    module: ShaderModule,
}

impl VertexShader {
    pub fn from_src_glsl(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::from_src_glsl(
                device,
                src,
                wgpu::ShaderStages::VERTEX,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }
    pub fn from_src_wgls(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(VertexShader {
            module: ShaderModule::from_src_wgls(device, src, DEFAULT_ENTRY_POINT, label)?,
        })
    }
    pub fn load(device: &wgpu::Device, path: &Path, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::load_glsl(
                device,
                path,
                wgpu::ShaderStages::VERTEX,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }

    pub fn vertex_state(&self) -> wgpu::VertexState {
        wgpu::VertexState {
            module: self,
            entry_point: self.entry_point(),
            buffers: &[],
        }
    }
}

#[derive(Debug, DerefMut)]
pub struct ComputeShader {
    module: ShaderModule,
}

impl ComputeShader {
    pub fn from_src_glsl(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::from_src_glsl(
                device,
                src,
                wgpu::ShaderStages::COMPUTE,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }
    pub fn from_src_wgls(device: &wgpu::Device, src: &str, label: Option<&str>) -> Result<Self> {
        Ok(ComputeShader {
            module: ShaderModule::from_src_wgls(device, src, DEFAULT_ENTRY_POINT, label)?,
        })
    }
    pub fn load_glsl(device: &wgpu::Device, path: &Path, label: Option<&str>) -> Result<Self> {
        Ok(Self {
            module: ShaderModule::load_glsl(
                device,
                path,
                wgpu::ShaderStages::COMPUTE,
                DEFAULT_ENTRY_POINT,
                label,
            )?,
        })
    }
}
