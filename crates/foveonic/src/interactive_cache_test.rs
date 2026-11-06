//! Checks exact scoped graph cuts, bounded ownership and source invalidation.

use super::*;
use crate::graph::Operation;

#[test]
fn parameter_resource_attribution_matches_the_unique_inventory() {
    let mut cache = Cache::default();
    let (graph, _, _, target) = source_graph("inventory", 16);
    cache.plan(&graph, &[target]).unwrap();
    let mut inventory = crate::recipe::RetainedBytes::default();
    cache.account_keys(&mut inventory);
    assert_eq!(
        cache
            .parameter_resources()
            .iter()
            .map(|role| role.payload_bytes)
            .sum::<u64>(),
        inventory.payload_bytes() as u64
    );
}

#[test]
#[ignore = "requires GPU"]
fn pressure_discards_old_cuts_and_uncached_descriptors_before_current_inputs() {
    let gpu = crate::gpu::Gpu::new().unwrap();
    let mut cache = Cache::default();
    for width in [16, 15] {
        let (graph, _, cut, _) = source_graph("cut", width);
        cache.plan_for(&graph, &[cut], true).unwrap();
        let value = Value {
            working_scale: crate::render::Scale::NATIVE,
            view: None,
            description: None,
            buffer: gpu
                .upload("immutable cut", &vec![1_f32; width as usize * 16 * 3])
                .unwrap(),
        };
        cache.put(&graph.outputs[cut].cache_name(), &value);
        assert!(cache.finish_seal());
    }
    assert_eq!(cache.entries.len(), 2);
    assert!(
        cache.trim_optional(true),
        "previous grid is optional under pressure"
    );
    assert_eq!(cache.entries.len(), 1);
    assert!(
        !cache.trim_optional(false),
        "the latest committed cut is protected"
    );
    let (graph, source, cut, target) = source_graph("new-scope", 15);
    cache.plan(&graph, &[target]).unwrap();
    cache.park();
    assert_eq!(
        cache.working.versions.len(),
        1,
        "uncached output descriptors have no parked owner"
    );
    cache.plan(&graph, &[target]).unwrap();
    let cached: Vec<_> = graph
        .outputs
        .iter()
        .map(|output| cache.get(&output.cache_name()).is_some())
        .collect();
    assert!(cached[cut]);
    assert!(
        !graph.required_until(&[target], &cached).unwrap()[source],
        "parking must retain exact source-free ancestry"
    );
}

fn source_graph(scope: &str, width: u32) -> (Graph, usize, usize, usize) {
    let mut graph = Graph::default();
    let source = graph.add_scoped(Operation::Source, &[], scope).unwrap();
    let region = graph
        .add_scoped(
            Operation::ImageRegion {
                prepared: crate::nodes::finish::image_region::Prepared::new(
                    [16, 16],
                    [0, 0, width, 16],
                    3,
                )
                .unwrap(),
                data: DataType::SensorLayers,
                output: "cut.input",
            },
            &[source],
            scope,
        )
        .unwrap();
    let cut = graph
        .add_scoped(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::SensorLayers,
                output: "cut.sensor",
            },
            &[region],
            scope,
        )
        .unwrap();
    graph.retain_for_seal(cut);
    let target = graph
        .add_scoped(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::SensorLayers,
                output: "view.result",
            },
            &[cut],
            scope,
        )
        .unwrap();
    (graph, source, cut, target)
}

#[test]
fn structural_identity_ignores_scope_and_keeps_exact_ancestry() {
    let mut cache = Cache::default();
    let (first, _, cut, target) = source_graph("first", 16);
    cache.plan(&first, &[target]).unwrap();
    let retained = cache.working.versions[&first.outputs[cut].cache_name()].clone();
    cache.working = WorkingSet::default();
    cache.finish();
    let (second, _, cut, target) = source_graph("different-scope", 16);
    cache.plan(&second, &[target]).unwrap();
    assert_eq!(
        cache.working.versions[&second.outputs[cut].cache_name()].generation,
        retained.generation
    );
    let (changed, _, cut, target) = source_graph("different-scope", 15);
    cache.plan(&changed, &[target]).unwrap();
    assert_ne!(
        cache.working.versions[&changed.outputs[cut].cache_name()].generation,
        retained.generation
    );
}

#[test]
fn structural_hash_collision_still_checks_exact_parameters_and_output_roles() {
    let mut cache = Cache::default();
    let recipe = |amount, role| Recipe {
        operation: std::mem::discriminant(&Operation::Source),
        parameters: Key::pod(&amount),
        data: DataType::SensorLayers,
        role,
        view: None,
        inputs: Vec::new(),
    };
    let a = cache.intern(recipe(0.25_f32, "test"), Vec::new());
    let b = cache.intern(recipe(0.5_f32, "test"), Vec::new());
    let same = cache.intern(recipe(0.25_f32, "test"), Vec::new());
    let other_role = cache.intern(recipe(0.25_f32, "another-output"), Vec::new());
    assert_eq!(cache.identities.len(), 2);
    assert!(Arc::ptr_eq(&a, &same));
    assert_ne!(a.generation, b.generation);
    assert_ne!(a.generation, other_role.generation);
}

#[test]
#[ignore = "requires GPU"]
fn sealed_cut_matches_across_scopes_and_is_invalidated_by_upstream_parameters() {
    let gpu = crate::gpu::Gpu::new().unwrap();
    let mut cache = Cache::default();
    let (graph, source, cut, target) = source_graph("main", 16);
    cache.plan_for(&graph, &[cut], true).unwrap();
    let value = Value {
        buffer: gpu
            .upload("Sealed sensor", &[100_f32; 16 * 16 * 3])
            .unwrap(),
        description: None,
        working_scale: crate::render::Scale::NATIVE,
        view: None,
    };
    let native = Value {
        buffer: gpu
            .upload("Temporary native", &[5_f32; 16 * 16 * 3])
            .unwrap(),
        description: None,
        working_scale: crate::render::Scale::NATIVE,
        view: None,
    };
    let region = graph.nodes[graph.outputs[cut].producer].inputs[0];
    cache.put(&graph.outputs[region].cache_name(), &native);
    cache.put(&graph.outputs[cut].cache_name(), &value);
    assert!(cache.finish_seal());
    assert_eq!(cache.bytes(), value.buffer.size());
    assert!(cache.entries.iter().all(|entry| entry.pinned));
    let (auxiliary, auxiliary_source, auxiliary_cut, auxiliary_target) =
        source_graph("appended-analysis", 16);
    cache.plan(&auxiliary, &[auxiliary_target]).unwrap();
    let cached: Vec<_> = auxiliary
        .outputs
        .iter()
        .map(|output| cache.get(&output.cache_name()).is_some())
        .collect();
    let needed = auxiliary
        .required_until(&[auxiliary_target], &cached)
        .unwrap();
    assert!(!needed[auxiliary_source]);
    assert!(cache
        .get(&auxiliary.outputs[auxiliary_cut].cache_name())
        .unwrap()
        .buffer
        .shares_storage(&value.buffer));
    let (changed, changed_source, _, changed_target) = source_graph("appended-analysis", 15);
    cache.plan(&changed, &[changed_target]).unwrap();
    let cached: Vec<_> = changed
        .outputs
        .iter()
        .map(|output| cache.get(&output.cache_name()).is_some())
        .collect();
    assert!(changed.required_until(&[changed_target], &cached).unwrap()[changed_source]);
    assert_eq!(
        cache.bytes(),
        value.buffer.size(),
        "unrelated graphs must not drop the pinned bank"
    );
    cache.plan(&graph, &[target]).unwrap();
    let cached: Vec<_> = graph
        .outputs
        .iter()
        .map(|output| cache.get(&output.cache_name()).is_some())
        .collect();
    assert!(!graph.required_until(&[target], &cached).unwrap()[source]);
}

#[test]
fn verified_source_reload_reuses_identity_but_memory_and_engine_changes_do_not() {
    use crate::source::{FileIdentity, SourceOrigin};
    let origin = SourceOrigin::File(FileIdentity {
        location: b"canonical/source.x3f".to_vec(),
        length: 123,
        modified_seconds: 456,
        modified_nanos: 789,
    });
    let owner = Arc::new(());
    let mut cache = Cache::default();
    cache.bind(&owner);
    cache.bind_source(&origin);
    let (graph, _, cut, target) = source_graph("main", 16);
    cache.plan(&graph, &[target]).unwrap();
    let retained = cache.working.versions[&graph.outputs[cut].cache_name()].clone();
    cache.bind_source(&origin.clone());
    cache.plan(&graph, &[target]).unwrap();
    assert_eq!(
        retained.generation,
        cache.working.versions[&graph.outputs[cut].cache_name()].generation
    );
    let memory = SourceOrigin::default();
    cache.bind_source(&memory);
    assert!(cache.working.versions.is_empty());
    cache.plan(&graph, &[target]).unwrap();
    let same_memory = cache.working.versions[&graph.outputs[cut].cache_name()].clone();
    cache.bind_source(&memory.clone());
    cache.plan(&graph, &[target]).unwrap();
    assert!(Arc::ptr_eq(
        &same_memory,
        &cache.working.versions[&graph.outputs[cut].cache_name()]
    ));
    cache.bind_source(&SourceOrigin::default());
    assert!(cache.working.versions.is_empty());
    cache.bind_source(&origin);
    cache.plan(&graph, &[target]).unwrap();
    cache.bind(&Arc::new(()));
    assert!(cache.working.versions.is_empty());
    assert!(cache.source.is_none());
}

#[test]
#[ignore = "requires GPU"]
fn optional_frontier_history_recovers_an_older_geometry_without_a_quality_label() {
    let gpu = crate::gpu::Gpu::new().unwrap();
    let mut cache = Cache::default();
    let mut held = Vec::new();
    for width in [16, 15] {
        let (mut graph, _, cut, target) = source_graph("same-request-root", width);
        graph.sealed_outputs.clear();
        graph.retain_for_session(cut);
        cache.plan(&graph, &[target]).unwrap();
        let value = Value {
            buffer: gpu
                .upload(
                    "Historical input",
                    &vec![width as f32; width as usize * 16 * 3],
                )
                .unwrap(),
            description: None,
            working_scale: crate::render::Scale::NATIVE,
            view: None,
        };
        cache.put(&graph.outputs[cut].cache_name(), &value);
        cache.finish();
        held.push(value);
    }
    let (graph, _, cut, target) = source_graph("another-scope", 16);
    cache.plan(&graph, &[target]).unwrap();
    let recovered = cache.get(&graph.outputs[cut].cache_name()).unwrap();
    assert!(recovered.buffer.shares_storage(&held[0].buffer));
    assert!(!recovered.buffer.shares_storage(&held[1].buffer));
}
