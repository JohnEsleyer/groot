use goscript::vm::VirtualMachine;
use hecs::World;

pub trait GrootPlugin {
    fn name(&self) -> &'static str;
    fn register_script_bindings(&self, vm: &mut VirtualMachine);
    fn update(&mut self, _world: &mut World, _dt: f64) {}
}

#[derive(Default)]
pub struct PluginManager {
    plugins: Vec<Box<dyn GrootPlugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    pub fn add<P: GrootPlugin + 'static>(&mut self, plugin: P) {
        log::info!("[GROOT PLUGIN] Loaded plugin '{}'", plugin.name());
        self.plugins.push(Box::new(plugin));
    }

    pub fn register_all_script_bindings(&self, vm: &mut VirtualMachine) {
        for plugin in &self.plugins {
            plugin.register_script_bindings(vm);
        }
    }

    pub fn update_all(&mut self, world: &mut World, dt: f64) {
        for plugin in &mut self.plugins {
            plugin.update(world, dt);
        }
    }
}