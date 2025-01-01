use wgpu::{Device, ShaderModule, ShaderModuleDescriptor};

pub struct Program {
    module: ShaderModule,
}

impl Program {
    pub fn new(device: &Device, data: ShaderModuleDescriptor) -> Self {
        let module = device.create_shader_module(data);

        Self { module }
    }

    pub fn get_shader(&self) -> &ShaderModule {
        &self.module
    }
}
