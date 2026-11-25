//! Checks exact scoped sensor data, bounded ownership and source invalidation.

use super::*;
use crate::graph::Operation;

fn admission_cache() -> Cache {
    let mut cache = Cache {
        frame: 10,
        ..Cache::default()
    };
    for name in [
        "higher",
        "incoming",
        "dehaze.result",
        "sensor",
        "old",
        "newer",
        "alias",
    ] {
        let version = cache.intern(
            Recipe {
                operation: std::mem::discriminant(&Operation::Source),
                parameters: Key::pod(&0_u32),
                data: DataType::SensorLayers,
                role: name,
                view: None,
                inputs: Vec::new(),
            },
            Vec::new(),
        );
        cache.planned.versions.insert(name.into(), version);
    }
    cache.planned.selected = ["higher", "incoming", "dehaze.result", "sensor", "alias"]
        .map(String::from)
        .into();
    cache
}

fn admission_value(gpu: &crate::gpu::Gpu, bytes: usize) -> Value {
    Value {
        view_baked: false,
        buffer: gpu
            .upload("Cache admission fixture", &vec![0_u32; bytes / 4])
            .unwrap(),
        description: None,
        working_scale: crate::render::Scale::NATIVE,
        view: None,
    }
}

fn retain_admission_value(cache: &mut Cache, name: &str, value: &Value, used: u64) {
    cache.entries.push(Entry {
        version: cache.planned.versions[name].clone(),
        value: value.clone(),
        used,
    });
}

#[test]
#[ignore = "requires GPU"]
fn existing_version_admission_keeps_its_original_buffer() {
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = admission_cache();
    let sensor = admission_value(&gpu, 64);
    let original = admission_value(&gpu, 32);
    let repeated = admission_value(&gpu, 64);
    retain_admission_value(&mut cache, "sensor", &sensor, 1);
    retain_admission_value(&mut cache, "incoming", &original, 2);

    cache.put("incoming", &repeated);

    assert_eq!(cache.bytes(), 96);
    assert_eq!(cache.entries.len(), 2);
    let entry = &cache.entries[1];
    assert!(entry.value.buffer.shares_storage(&original.buffer));
    assert!(!entry.value.buffer.shares_storage(&repeated.buffer));
    assert_eq!(entry.used, cache.frame);
    assert_eq!(cache.entries[0].used, 1);
}

/// A cache holding these restart inputs of one edit, stamped with its use.
pub(crate) fn restart_inputs(gpu: &crate::gpu::Gpu, names: &[&str], edit_use: u64) -> Cache {
    let mut cache = admission_cache();
    for name in names {
        retain_admission_value(&mut cache, name, &admission_value(gpu, 32), 1);
        let generation = cache.planned.versions[*name].generation;
        let image = cache.produced.entry(WHOLE_EDIT).or_default();
        image.versions.insert(generation);
        image.restart.insert(generation);
    }
    cache.restart_use = edit_use;
    cache
}

#[test]
#[ignore = "requires GPU"]
fn restart_inputs_are_not_optional_and_carry_their_edits_use() {
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = restart_inputs(&gpu, &["old", "newer"], 5);

    assert!(!cache.trim_optional(), "restart inputs are not optional");
    assert_eq!(cache.restart_use(), Some(5));
    assert!(cache.drop_restart_input());
    assert_eq!(cache.restart_use(), Some(5));
    assert!(cache.drop_restart_input());
    assert_eq!(cache.restart_use(), None);
    assert!(!cache.drop_restart_input());
}

#[test]
#[ignore = "requires GPU"]
fn each_image_keeps_one_version_and_its_own_restart_stages() {
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = admission_cache();
    for name in ["old", "newer", "higher"] {
        retain_admission_value(&mut cache, name, &admission_value(&gpu, 32), 1);
    }
    let generation = |cache: &Cache, name: &str| cache.planned.versions[name].generation;
    let held = |cache: &Cache, generation: u64| {
        let mut entries = cache.entries.iter();
        entries.any(|entry| entry.version.generation == generation)
    };
    let (old, newer, higher) = (
        generation(&cache, "old"),
        generation(&cache, "newer"),
        generation(&cache, "higher"),
    );
    // A detail patch's edit, then its whole image's, restart from other stages.
    cache.planned.restart = [old].into();
    cache.commit(true, Some(1));
    cache.planned.restart = [newer].into();
    cache.commit(false, Some(2));
    assert!(cache.is_restart_input(old) && cache.is_restart_input(newer));
    // A render that is not an edit displaces neither.
    cache.planned.restart = [higher].into();
    cache.commit(false, None);
    assert!(cache.is_restart_input(old) && cache.is_restart_input(newer));
    assert!(!cache.is_restart_input(higher));
    assert!(cache.trim_optional(), "the stage no edit restarts from");
    assert!(!cache.trim_optional());
    assert!(held(&cache, old) && held(&cache, newer) && !held(&cache, higher));

    // A new version of a stage stays beside the old one only until every
    // image that planned the old one has rendered again.
    let replacement = cache.intern(
        Recipe {
            operation: std::mem::discriminant(&Operation::Source),
            parameters: Key::pod(&1_u32),
            data: DataType::SensorLayers,
            role: "old",
            view: None,
            inputs: Vec::new(),
        },
        Vec::new(),
    );
    cache.planned.versions.insert("old".into(), replacement);
    for (detail, edit_use) in [(true, Some(3)), (false, None)] {
        cache.commit(detail, edit_use);
        assert!(held(&cache, old), "the whole image's edit still plans it");
    }
    cache.commit(false, Some(4));
    assert!(!held(&cache, old) && held(&cache, newer));

    cache
        .plan_for(&source_graph("sensor", 16).0, &[], true)
        .unwrap();
    cache.finish_sensor_data();
    assert!(
        cache.produced.is_empty(),
        "new sensor data starts every image over"
    );
}

#[test]
fn working_version_inventory_counts_unused_map_capacity() {
    let mut cache = admission_cache();
    let capacity_before = cache.planned.versions.capacity();
    let mut retained_before = crate::recipe::RetainedBytes::default();
    let bytes_before = cache.account_keys(&mut retained_before);

    cache.planned.versions.reserve(113);

    let capacity_after = cache.planned.versions.capacity();
    assert!(capacity_after > capacity_before);
    let mut retained_after = crate::recipe::RetainedBytes::default();
    let bytes_after = cache.account_keys(&mut retained_after);
    assert_eq!(
        bytes_after - bytes_before,
        (capacity_after - capacity_before) * std::mem::size_of::<(String, Arc<Version>)>()
    );
    assert_eq!(retained_after.bytes(), retained_before.bytes());
}

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
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = Cache::default();
    for width in [16, 15] {
        let (graph, _, sensor, _) = source_graph("sensor", width);
        cache.plan_for(&graph, &[sensor], true).unwrap();
        let value = Value {
            working_scale: crate::render::Scale::NATIVE,
            view_baked: false,
            view: None,
            description: None,
            buffer: gpu
                .upload(
                    "immutable sensor data",
                    &vec![1_f32; width as usize * 16 * 3],
                )
                .unwrap(),
        };
        cache.put(&graph.outputs[sensor].cache_name(), &value);
        assert!(cache.finish_sensor_data());
    }
    assert_eq!(cache.entries.len(), 2);
    assert!(
        cache.trim_optional(),
        "previous grid is optional under pressure"
    );
    assert_eq!(cache.entries.len(), 1);
    assert!(
        !cache.trim_optional(),
        "the latest committed sensor data is protected"
    );
    let (graph, source, sensor, target) = source_graph("new-scope", 15);
    cache.plan(&graph, &[target]).unwrap();
    let cached: Vec<_> = graph
        .outputs
        .iter()
        .map(|output| cache.get(&output.cache_name()).is_some())
        .collect();
    assert!(cached[sensor]);
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
                output: "sensor.input",
            },
            &[source],
            scope,
        )
        .unwrap();
    let sensor = graph
        .add_scoped(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::SensorLayers,
                output: "sensor.data",
            },
            &[region],
            scope,
        )
        .unwrap();
    graph.retain_as_sensor_data(sensor);
    let target = graph
        .add_scoped(
            Operation::ImageAlias {
                dehaze_basis: None,
                data: DataType::SensorLayers,
                output: "view.result",
            },
            &[sensor],
            scope,
        )
        .unwrap();
    (graph, source, sensor, target)
}

#[test]
fn structural_identity_ignores_scope_and_keeps_exact_ancestry() {
    let mut cache = Cache::default();
    let (first, _, sensor, target) = source_graph("first", 16);
    cache.plan(&first, &[target]).unwrap();
    let retained = cache.planned.versions[&first.outputs[sensor].cache_name()].clone();
    cache.planned = PlannedRecipe::default();
    cache.finish();
    let (second, _, sensor, target) = source_graph("different-scope", 16);
    cache.plan(&second, &[target]).unwrap();
    assert_eq!(
        cache.planned.versions[&second.outputs[sensor].cache_name()].generation,
        retained.generation
    );
    let (changed, _, sensor, target) = source_graph("different-scope", 15);
    cache.plan(&changed, &[target]).unwrap();
    assert_ne!(
        cache.planned.versions[&changed.outputs[sensor].cache_name()].generation,
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
fn sensor_data_matches_across_scopes_and_is_invalidated_by_upstream_parameters() {
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = Cache::default();
    let (graph, source, sensor, target) = source_graph("main", 16);
    cache.plan_for(&graph, &[sensor], true).unwrap();
    let value = Value {
        view_baked: false,
        buffer: gpu
            .upload("Prepared sensor data", &[100_f32; 16 * 16 * 3])
            .unwrap(),
        description: None,
        working_scale: crate::render::Scale::NATIVE,
        view: None,
    };
    let native = Value {
        view_baked: false,
        buffer: gpu
            .upload("Temporary native", &[5_f32; 16 * 16 * 3])
            .unwrap(),
        description: None,
        working_scale: crate::render::Scale::NATIVE,
        view: None,
    };
    let region = graph.nodes[graph.outputs[sensor].producer].inputs[0];
    cache.put(&graph.outputs[region].cache_name(), &native);
    cache.put(&graph.outputs[sensor].cache_name(), &value);
    assert!(cache.finish_sensor_data());
    assert_eq!(cache.bytes(), value.buffer.size());
    assert!(cache
        .entries
        .iter()
        .all(|entry| cache.sensor_data.contains(&entry.version.generation)));
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
    let (graph, _, sensor, target) = source_graph("main", 16);
    cache.plan(&graph, &[target]).unwrap();
    let retained = cache.planned.versions[&graph.outputs[sensor].cache_name()].clone();
    cache.bind_source(&origin.clone());
    cache.plan(&graph, &[target]).unwrap();
    assert_eq!(
        retained.generation,
        cache.planned.versions[&graph.outputs[sensor].cache_name()].generation
    );
    let memory = SourceOrigin::default();
    cache.bind_source(&memory);
    assert!(cache.planned.versions.is_empty());
    cache.plan(&graph, &[target]).unwrap();
    let same_memory = cache.planned.versions[&graph.outputs[sensor].cache_name()].clone();
    cache.bind_source(&memory.clone());
    cache.plan(&graph, &[target]).unwrap();
    assert!(Arc::ptr_eq(
        &same_memory,
        &cache.planned.versions[&graph.outputs[sensor].cache_name()]
    ));
    cache.bind_source(&SourceOrigin::default());
    assert!(cache.planned.versions.is_empty());
    cache.bind_source(&origin);
    cache.plan(&graph, &[target]).unwrap();
    cache.bind(&Arc::new(()));
    assert!(cache.planned.versions.is_empty());
    assert!(cache.source.is_none());
}

#[test]
#[ignore = "requires GPU"]
fn optional_frontier_history_recovers_an_older_geometry_without_a_quality_label() {
    let gpu = crate::gpu::Gpu::test_executor();
    let mut cache = Cache::default();
    let mut held = Vec::new();
    for width in [16, 15] {
        let (mut graph, _, sensor, target) = source_graph("same-request-root", width);
        graph.sensor_data_outputs.clear();
        graph.retain_for_session(sensor);
        cache.plan(&graph, &[target]).unwrap();
        let value = Value {
            view_baked: false,
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
        cache.put(&graph.outputs[sensor].cache_name(), &value);
        cache.finish();
        held.push(value);
    }
    let (graph, _, sensor, target) = source_graph("another-scope", 16);
    cache.plan(&graph, &[target]).unwrap();
    let recovered = cache.get(&graph.outputs[sensor].cache_name()).unwrap();
    assert!(recovered.buffer.shares_storage(&held[0].buffer));
    assert!(!recovered.buffer.shares_storage(&held[1].buffer));
}
