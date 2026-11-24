//! Owned observation state. This module never dereferences Vulkan pointers.
use serde_json::{json, Value};
use std::collections::HashMap;

pub const TARGET: &str = "5b112e9919dff8911604f606a636b324e197cde9797196efd604b5f9ca48f9c7";
const MAX_OBJECTS: usize = 65536;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Identity {
    Target,
    Other,
    Unknown,
}

#[derive(Clone, Copy)]
pub struct Shader {
    pub identity: Identity,
    pub references: u64,
}

#[derive(Clone, Copy)]
pub struct Pipeline {
    pub id: u64,
    pub identity: Identity,
    pub references: u64,
}

#[derive(Clone)]
pub struct Command {
    pub id: u64,
    pub pool: u64,
    pub epoch: u64,
    pub primary: bool,
    pub pipeline: Option<Pipeline>,
    pub target_dispatches: u64,
    pub unknown: bool,
    pub ready: bool,
}

#[derive(Default)]
pub struct Tracker {
    serial: u64,
    pub shaders: HashMap<(u64, u64), Shader>,
    pub pipelines: HashMap<(u64, u64), Pipeline>,
    pub commands: HashMap<(u64, u64), Command>,
    pub unsupported: u64,
    pub extension_queries: u64,
    pub recorded: u64,
    pub submitted: u64,
    pub observation_disabled: bool,
}

impl Tracker {
    fn id(&mut self) -> u64 {
        self.serial = self.serial.saturating_add(1);
        self.serial
    }
    pub fn unknown(&mut self) {
        self.unsupported = self.unsupported.saturating_add(1);
    }
    fn room(&mut self) -> bool {
        if self.shaders.len() + self.pipelines.len() + self.commands.len() >= MAX_OBJECTS {
            self.observation_disabled = true;
            self.unknown();
        }
        !self.observation_disabled
    }
    pub fn shader(&mut self, device: u64, handle: u64, hash: Option<&str>) -> Option<Value> {
        if !self.room() {
            return None;
        }
        let identity = match hash {
            Some(TARGET) => Identity::Target,
            Some(_) => Identity::Other,
            None => Identity::Unknown,
        };
        if identity == Identity::Unknown {
            self.unknown();
        }
        if let Some(existing) = self.shaders.get_mut(&(device, handle)) {
            existing.references = existing.references.saturating_add(1);
            if existing.identity != identity {
                existing.identity = Identity::Unknown;
                self.unknown();
                return None;
            }
        } else {
            self.shaders.insert(
                (device, handle),
                Shader {
                    identity,
                    references: 1,
                },
            );
        }
        (identity == Identity::Target)
            .then(|| json!({"event":"target_shader","shader_sha256":TARGET}))
    }
    pub fn pipeline(
        &mut self,
        device: u64,
        handle: u64,
        module: u64,
        unsupported_stage: bool,
    ) -> Option<Value> {
        if !self.room() {
            return None;
        }
        let identity = if unsupported_stage {
            Identity::Unknown
        } else {
            self.shaders
                .get(&(device, module))
                .map(|s| s.identity)
                .unwrap_or(Identity::Unknown)
        };
        if identity == Identity::Unknown {
            self.unknown();
        }
        let mut conflict = false;
        let pipeline = if let Some(existing) = self.pipelines.get_mut(&(device, handle)) {
            existing.references = existing.references.saturating_add(1);
            if existing.identity != identity {
                existing.identity = Identity::Unknown;
                conflict = true;
            }
            *existing
        } else {
            Pipeline {
                id: self.id(),
                identity,
                references: 1,
            }
        };
        if conflict {
            self.unknown();
        }
        if pipeline.identity == Identity::Unknown {
            for c in self.commands.values_mut() {
                if c.pipeline.is_some_and(|p| p.id == pipeline.id) {
                    c.pipeline = Some(pipeline);
                    c.unknown = true;
                }
            }
        }
        self.pipelines.insert((device, handle), pipeline);
        (pipeline.identity == Identity::Target).then(
            || json!({"event":"target_pipeline","pipeline_id":pipeline.id,"shader_sha256":TARGET}),
        )
    }
    pub fn retire_shader(&mut self, device: u64, handle: u64) {
        if let Some(s) = self.shaders.get_mut(&(device, handle)) {
            s.references -= 1;
            if s.references == 0 {
                self.shaders.remove(&(device, handle));
            }
        }
    }
    pub fn retire_pipeline(&mut self, device: u64, handle: u64) {
        if let Some(p) = self.pipelines.get_mut(&(device, handle)) {
            p.references -= 1;
            if p.references == 0 {
                self.pipelines.remove(&(device, handle));
            }
        }
    }
    pub fn allocate(&mut self, device: u64, handle: u64, pool: u64, primary: bool) {
        if !self.room() {
            return;
        }
        let cmd = Command {
            id: self.id(),
            pool,
            epoch: 0,
            primary,
            pipeline: None,
            target_dispatches: 0,
            unknown: !primary,
            ready: false,
        };
        self.commands.insert((device, handle), cmd);
    }
    pub fn reset(&mut self, device: u64, handle: u64) {
        if let Some(c) = self.commands.get_mut(&(device, handle)) {
            c.epoch = c.epoch.saturating_add(1);
            c.pipeline = None;
            c.target_dispatches = 0;
            c.unknown = !c.primary;
            c.ready = false;
        } else {
            self.unknown();
        }
    }
    pub fn reset_pool(&mut self, device: u64, pool: u64) {
        let handles: Vec<u64> = self
            .commands
            .iter()
            .filter_map(|(&(d, h), c)| (d == device && c.pool == pool).then_some(h))
            .collect();
        for h in handles {
            self.reset(device, h);
        }
    }
    pub fn end(&mut self, device: u64, command: u64, success: bool) {
        if let Some(c) = self.commands.get_mut(&(device, command)) {
            c.ready = success;
        }
        if !success {
            self.mark_command_unknown(device, command);
        }
    }
    pub fn unknown_pool(&mut self, device: u64, pool: u64) {
        self.unknown();
        for (&(d, _), c) in &mut self.commands {
            if d == device && c.pool == pool {
                c.unknown = true;
            }
        }
    }
    pub fn destroy_pool(&mut self, device: u64, pool: u64) {
        self.commands
            .retain(|&(d, _), c| d != device || c.pool != pool);
    }
    pub fn destroy_device(&mut self, device: u64) {
        self.shaders.retain(|&(d, _), _| d != device);
        self.pipelines.retain(|&(d, _), _| d != device);
        self.commands.retain(|&(d, _), _| d != device);
    }
    pub fn bind(&mut self, device: u64, command: u64, pipeline: u64) {
        let p = self.pipelines.get(&(device, pipeline)).copied();
        if let Some(c) = self.commands.get_mut(&(device, command)) {
            c.pipeline = p;
        } else {
            self.unknown();
        }
    }
    pub fn dispatch(&mut self, device: u64, command: u64, groups: [u32; 3]) -> Option<Value> {
        let Some(c) = self.commands.get_mut(&(device, command)) else {
            self.unknown();
            return None;
        };
        let Some(p) = c.pipeline else {
            c.unknown = true;
            self.unknown();
            return None;
        };
        if p.identity != Identity::Target {
            if p.identity == Identity::Unknown {
                c.unknown = true;
                self.unknown();
            }
            return None;
        }
        c.target_dispatches = c.target_dispatches.saturating_add(1);
        self.recorded = self.recorded.saturating_add(1);
        Some(
            json!({"event":"target_dispatch_recorded","command_id":c.id,"recording_epoch":c.epoch,
            "pipeline_id":p.id,"groups":groups,"primary":c.primary,"command_unknown":c.unknown,
            "descriptor_contents_verified":false,"execution_proven":false}),
        )
    }
    pub fn mark_command_unknown(&mut self, device: u64, command: u64) {
        self.unknown();
        if let Some(c) = self.commands.get_mut(&(device, command)) {
            c.unknown = true;
        }
    }
    pub fn shader_object_bind(&mut self, device: u64, command: u64) {
        self.mark_command_unknown(device, command);
        if let Some(c) = self.commands.get_mut(&(device, command)) {
            c.pipeline = None;
        }
    }
    pub fn submission(
        &mut self,
        device: u64,
        commands: &[u64],
        success: bool,
        complete: bool,
    ) -> Option<Value> {
        let mut records = Vec::new();
        let mut unknown = !complete;
        let mut target = 0u64;
        for h in commands {
            if let Some(c) = self.commands.get(&(device, *h)) {
                unknown |= c.unknown || !c.ready;
                target = target.saturating_add(c.target_dispatches);
                if c.target_dispatches > 0 {
                    records.push(json!({"command_id":c.id,"recording_epoch":c.epoch,"target_dispatches":c.target_dispatches}));
                }
            } else {
                unknown = true;
            }
        }
        if unknown {
            self.unknown();
        }
        if target == 0 {
            return None;
        }
        if success {
            self.submitted = self.submitted.saturating_add(target);
        }
        Some(
            json!({"event":"target_submission","submission_id":self.id(),"success":success,
            "commands":records,"unknown_commands":unknown,"command_list_complete":complete,"execution_proven":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_destruction_and_handle_reuse_do_not_rewrite_pipeline_identity() {
        let mut t = Tracker::default();
        t.shader(1, 2, Some(TARGET));
        t.pipeline(1, 3, 2, false);
        let first = t.pipelines[&(1, 3)].id;
        t.retire_shader(1, 2);
        t.shader(1, 2, Some("other"));
        assert_eq!(t.pipelines[&(1, 3)].identity, Identity::Target);
        t.retire_pipeline(1, 3);
        t.pipeline(1, 3, 2, false);
        assert_eq!(t.pipelines[&(1, 3)].identity, Identity::Other);
        assert_ne!(t.pipelines[&(1, 3)].id, first);
    }
    #[test]
    fn reset_and_failed_submission_do_not_claim_execution() {
        let mut t = Tracker::default();
        t.shader(1, 2, Some(TARGET));
        t.pipeline(1, 3, 2, false);
        t.allocate(1, 4, 5, true);
        t.reset(1, 4);
        t.bind(1, 4, 3);
        t.dispatch(1, 4, [1, 1, 1]);
        t.end(1, 4, true);
        let failed = t.submission(1, &[4], false, true).unwrap();
        assert_eq!(failed["success"], false);
        assert_eq!(t.submitted, 0);
        t.submission(1, &[4], true, true);
        assert_eq!(t.submitted, 1);
        t.reset_pool(1, 5);
        assert!(t.submission(1, &[4], true, true).is_none());
        assert_eq!(t.commands[&(1, 4)].epoch, 2);
        t.destroy_pool(1, 5);
        assert!(t.commands.is_empty());
    }
    #[test]
    fn unsupported_identity_is_not_treated_as_other() {
        let mut t = Tracker::default();
        t.pipeline(1, 3, 999, false);
        assert_eq!(t.pipelines[&(1, 3)].identity, Identity::Unknown);
        assert_eq!(t.unsupported, 1);
        t.allocate(1, 4, 5, true);
        t.mark_command_unknown(1, 4);
        assert!(t.commands[&(1, 4)].unknown);
    }
    #[test]
    fn failed_end_and_pool_reset_taint_submission() {
        let mut t = Tracker::default();
        t.shader(1, 2, Some(TARGET));
        t.pipeline(1, 3, 2, false);
        t.allocate(1, 4, 5, true);
        t.reset(1, 4);
        t.bind(1, 4, 3);
        t.dispatch(1, 4, [1, 1, 1]);
        t.end(1, 4, false);
        assert_eq!(
            t.submission(1, &[4], false, true).unwrap()["unknown_commands"],
            true
        );
        t.reset(1, 4);
        t.bind(1, 4, 3);
        t.dispatch(1, 4, [1, 1, 1]);
        t.end(1, 4, true);
        t.unknown_pool(1, 5);
        assert_eq!(
            t.submission(1, &[4], true, true).unwrap()["unknown_commands"],
            true
        );
    }
    #[test]
    fn object_limit_disables_observation() {
        let mut t = Tracker::default();
        for h in 0..MAX_OBJECTS as u64 {
            t.shader(1, h, Some("other"));
        }
        t.shader(1, MAX_OBJECTS as u64, Some(TARGET));
        assert!(t.observation_disabled);
        assert_eq!(t.shaders.len(), MAX_OBJECTS);
    }
    #[test]
    fn duplicate_live_handles_survive_one_destroy_and_conflicts_are_unknown() {
        let mut t = Tracker::default();
        t.shader(1, 2, Some(TARGET));
        t.shader(1, 2, Some(TARGET));
        t.retire_shader(1, 2);
        assert_eq!(t.shaders[&(1, 2)].identity, Identity::Target);
        t.pipeline(1, 3, 2, false);
        let id = t.pipelines[&(1, 3)].id;
        t.pipeline(1, 3, 2, false);
        t.retire_pipeline(1, 3);
        assert_eq!(t.pipelines[&(1, 3)].id, id);
        t.allocate(1, 4, 5, true);
        t.bind(1, 4, 3);
        t.pipeline(1, 3, 999, false);
        assert_eq!(t.pipelines[&(1, 3)].identity, Identity::Unknown);
        assert!(t.commands[&(1, 4)].unknown);
        t.retire_pipeline(1, 3);
        t.retire_pipeline(1, 3);
        assert!(t.pipelines.is_empty());
    }
    #[test]
    fn unknown_dispatch_and_partial_submit_cannot_look_complete() {
        let mut t = Tracker::default();
        t.shader(1, 2, Some(TARGET));
        t.pipeline(1, 3, 2, false);
        t.pipeline(1, 6, 999, false);
        t.allocate(1, 4, 5, true);
        t.reset(1, 4);
        t.bind(1, 4, 6);
        assert!(t.dispatch(1, 4, [1, 1, 1]).is_none());
        t.bind(1, 4, 3);
        t.dispatch(1, 4, [1, 1, 1]);
        t.end(1, 4, true);
        assert_eq!(
            t.submission(1, &[4], true, true).unwrap()["unknown_commands"],
            true
        );
        t.reset(1, 4);
        t.bind(1, 4, 3);
        t.dispatch(1, 4, [1, 1, 1]);
        t.end(1, 4, true);
        let v = t.submission(1, &[4], true, false).unwrap();
        assert_eq!(v["unknown_commands"], true);
        assert_eq!(v["command_list_complete"], false);
    }
}
