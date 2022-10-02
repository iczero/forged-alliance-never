pub mod simulation;

use bevy_ecs::prelude::*;
use simulation::*;

/// total resource cost to build paragon
const PARAGON_DAMAGE: Damage = Damage {
    mass_total: 250_200.0,
    energy_total: 7_506_000.0,
    build_time: 325_000.0,
    health: 0.0,
    health_points: 5_000,
};
/// total resource cost to build sacrifice-enabled RAS SACU
const RAS_SACU_DAMAGE: Damage = Damage {
    // mass_total: 6_600.0,
    // energy_total: 119_600.0,
    mass_total: 6_450.0,
    energy_total: 117_100.0,
    // build_time: 23_500.0,
    build_time: 22_800.0,
    health: 0.0,
    health_points: 15_000,
};
/// RAS SACU resource production
const RAS_SACU_RESOURCE_PRODUCTION: ResourceProducer = ResourceProducer {
    mass_yield: 11.0 / TICK_RATE,
    energy_yield: 1_020.0 / TICK_RATE,
};
/// RAS SACU sacrifice
const RAS_SACU_SACRIFICE: SacrificeCapable = SacrificeCapable {
    mass_efficiency: 0.9,
    energy_efficiency: 0.9,
};
/// RAS SACU engineering power
const RAS_SACU_ENGINEERING: Engineering = Engineering {
    build_rate: 56.0 / TICK_RATE,
};

#[derive(Component)]
pub struct QuantumGate {
    /// time (in ticks) for unit being constructed to exit the factory
    pub rolloff_time: i32,
    /// time (in ticks) left for unit to leave
    pub rolloff_current: i32,
    // bundle for new unit
    // this unfortunately does not work
    // unit_bundle: Box<dyn Bundle>
}

impl Default for QuantumGate {
    fn default() -> Self {
        QuantumGate {
            rolloff_time: 15,
            rolloff_current: 0
        }
    }
}

#[derive(Component)]
pub struct RASSupportCommander;

#[derive(Component)]
pub struct Paragon;

#[derive(Component)]
pub struct SacrificeCapable {
    pub mass_efficiency: f64,
    pub energy_efficiency: f64,
}

#[derive(Component)]
pub struct Sacrificing {
    pub target: Entity,
}

pub fn quantum_gate_spawn_construct(
    mut query: Query<
        (Entity, &mut QuantumGate),
        (
            With<Executing>,
            Without<ConstructionPaused>,
            Without<Constructing>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mut quantum_gate) in &mut query {
        if quantum_gate.rolloff_current > 0 {
            // tick rolloff
            quantum_gate.rolloff_current -= 1;
            continue;
        } else if quantum_gate.rolloff_current == 0 {
            quantum_gate.rolloff_current = -1;
            // spawn new RAS SACU and begin construction
            let construct_target = commands
                .spawn()
                .insert(RASSupportCommander)
                .insert(RAS_SACU_DAMAGE)
                .insert(RAS_SACU_ENGINEERING)
                .insert(WillExecuteOnConstruct)
                .insert(RAS_SACU_RESOURCE_PRODUCTION)
                .insert(ResourceConsumer {
                    mass_request: 0.0,
                    mass_consumed: 0.0,
                    energy_request: 0.0,
                    energy_consumed: 0.0,
                })
                .insert(RAS_SACU_SACRIFICE)
                .id();

            commands.entity(entity).insert(Constructing {
                target: construct_target,
                build_amount: 0.0,
                mass_requested: 0.0,
                energy_requested: 0.0,
                mass_consumption_multiplier: 1.0,
                energy_consumption_multiplier: 1.0,
            });
        } else {
            // construction finished or cancelled
            quantum_gate.rolloff_current = quantum_gate.rolloff_time;
        }
    }
}

pub fn construct_sacrifice(
    mut param_set: ParamSet<(
        Query<
            (Entity, &Damage, &Sacrificing, &SacrificeCapable),
            (With<Executing>, Without<ConstructionPaused>),
        >,
        Query<&mut Damage>,
    )>,
    mut commands: Commands,
) {
    struct SacrificeInfo {
        source_entity: Entity,
        mass_available: f64,
        energy_available: f64,
        target_entity: Entity,
    }
    // i will have to... allocate
    let mut sacrifice_list: Vec<SacrificeInfo> = Vec::new();
    for (entity, damage, sacrificing, sacrifice_capability) in &mut param_set.p0() {
        sacrifice_list.push(SacrificeInfo {
            source_entity: entity,
            mass_available: damage.mass_total
                * damage.health
                * sacrifice_capability.mass_efficiency,
            energy_available: damage.energy_total
                * damage.health
                * sacrifice_capability.energy_efficiency,
            target_entity: sacrificing.target,
        });
    }
    let mut target_query = param_set.p1();
    for sacrificing in &mut sacrifice_list {
        if let Ok(mut target_damage) = target_query.get_mut(sacrificing.target_entity) {
            if target_damage.health >= 1.0 {
                // target finished
                commands
                    .entity(sacrificing.source_entity)
                    .remove::<Sacrificing>();
                continue;
            } else {
                // contribute build and despawn self
                target_damage.health += f64::min(
                    sacrificing.mass_available / target_damage.mass_total,
                    sacrificing.energy_available / target_damage.energy_total,
                );
                commands.entity(sacrificing.source_entity).despawn();
            }
        } else {
            // target gone
            commands
                .entity(sacrificing.source_entity)
                .remove::<Sacrificing>();
        }
    }
}

pub struct RASSimulation {
    pub world: World,
    pub update_schedule: Schedule,
}

impl RASSimulation {
    pub fn new() -> Self {
        let mut world = World::new();

        // resources
        world.insert_resource(CurrentTick(0));
        world.insert_resource(Economy {
            mass_capacity: 40000.0,
            energy_capacity: 100000.0,
            ..Default::default()
        });
        world.insert_resource(LogHandler::new(|message| println!("{}", message)));

        // schedule and stages
        let mut schedule = Schedule::default();
        let tick_stage = SystemStage::single_threaded().with_system(count_tick);
        let unit_spawn_stage = SystemStage::parallel().with_system(quantum_gate_spawn_construct);
        let update_stage = SystemStage::parallel()
            .with_system(execute_on_finished_construction)
            .with_system(do_construct_resources_request)
            .with_system(construct_sacrifice);
        let economy_request_stage = SystemStage::parallel()
            .with_system(economy_resource_producers)
            .with_system(economy_process_resource_requests.after(economy_resource_producers));
        let resource_usage_stage = SystemStage::parallel().with_system(do_construct);
        let economy_accounting_stage =
            SystemStage::parallel().with_system(economy_process_resource_consumption);

        schedule.add_stage("tick count", tick_stage);
        schedule.add_stage("unit spawning", unit_spawn_stage);
        schedule.add_stage("update", update_stage);
        schedule.add_stage("economy request", economy_request_stage);
        schedule.add_stage("resource usage", resource_usage_stage);
        schedule.add_stage("economy accounting", economy_accounting_stage);

        RASSimulation {
            world,
            update_schedule: schedule,
        }
    }

    pub fn run(&mut self) {
        self.update_schedule.run(&mut self.world);
    }

    pub fn print_economy(&mut self) {
        let economy = self.world.get_resource::<Economy>().unwrap();
        println!("Economy info:");
        println!(
            "  Mass: {:.2}/{} +{:.4} -{:.4} (stall {:.5}, actual {:+.4})",
            economy.mass,
            economy.mass_capacity,
            economy.mass_produced,
            economy.mass_requested,
            economy.mass_stall,
            economy.mass_produced - economy.mass_consumed
        );
        println!(
            "  Energy: {:.2}/{} +{:.4} -{:.4} (stall {:.5}, actual {:+.4})",
            economy.energy,
            economy.energy_capacity,
            economy.energy_produced,
            economy.energy_requested,
            economy.energy_stall,
            economy.energy_produced - economy.energy_consumed
        );
    }
}

fn main() {
    println!("Hello, world!");
    let mut sim = RASSimulation::new();

    let gate = sim
        .world
        .spawn()
        .insert(QuantumGate::default())
        .insert(Executing)
        .insert(ResourceConsumer::default())
        .insert(Engineering {
            build_rate: 120000.0,
        })
        .id();

    let _resource_producer = sim
        .world
        .spawn()
        .insert(ResourceProducer {
            mass_yield: 200.0,
            energy_yield: 3_800.0,
        })
        .insert(Executing)
        .id();

    let mut sacu_query = sim.world.query::<&RASSupportCommander>();
    for _ in 0..1000 {
        sim.run();
        println!(
            "Tick {}",
            sim.world.get_resource::<CurrentTick>().unwrap().0
        );
        if let Some(constructing) = sim.world.entity(gate).get::<Constructing>() {
            println!(
                "Quantum gate constructing entity id {}",
                constructing.target.id()
            );
            if let Some(damage) = sim.world.entity(constructing.target).get::<Damage>() {
                println!("  Build progress: {:.2}%", damage.health * 100.0);
            }
        }
        let mut sacu_count = 0;
        for _ in sacu_query.iter(&sim.world) {
            sacu_count += 1;
        }
        println!("There are currently {} SACUs", sacu_count);
        sim.print_economy();
    }
}
