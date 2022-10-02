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
    total_mass: 0.0,
    total_energy: 0.0,
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
            rolloff_current: 0,
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
                target_damage.health = target_damage.health.min(1.0);
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

    pub fn get_tick(&self) -> u64 {
        self.world.get_resource::<CurrentTick>().unwrap().0
    }

    pub fn print_tick(&self) {
        println!(
            "Tick {}",
            self.world.get_resource::<CurrentTick>().unwrap().0
        );
    }

    pub fn print_economy(&self) {
        let economy = self.world.get_resource::<Economy>().unwrap();
        println!("Economy info:");
        println!(
            "  Mass: {:.2}/{} +{:.4} -{:.4} (stall {:.5}, actual {:+.4})",
            economy.mass,
            economy.mass_capacity,
            economy.mass_produced * TICK_RATE,
            economy.mass_requested * TICK_RATE,
            economy.mass_stall,
            (economy.mass_produced - economy.mass_consumed) * TICK_RATE
        );
        println!(
            "  Energy: {:.2}/{} +{:.4} -{:.4} (stall {:.5}, actual {:+.4})",
            economy.energy,
            economy.energy_capacity,
            economy.energy_produced * TICK_RATE,
            economy.energy_requested * TICK_RATE,
            economy.energy_stall,
            (economy.energy_produced - economy.energy_consumed) * TICK_RATE
        );
    }
}

fn main() {
    println!("Hello, world!");
    let mut sim = RASSimulation::new();

    let args = std::env::args().collect::<Vec<String>>();
    let target_count = args
        .get(1)
        .expect("requires sacu count")
        .parse::<u32>()
        .expect("invalid number");
    let mass_yield = args
        .get(2)
        .expect("requires initial mass income")
        .parse::<f64>()
        .expect("invalid number");

    let gate = sim
        .world
        .spawn()
        .insert(QuantumGate::default())
        .insert(Executing)
        .insert(ResourceConsumer::default())
        .insert(Engineering {
            build_rate: 120000.0 / TICK_RATE,
        })
        .id();

    let _resource_producer = sim
        .world
        .spawn()
        .insert(ResourceProducer {
            mass_yield: mass_yield / TICK_RATE,
            energy_yield: 100_000.0 / TICK_RATE,
            ..Default::default()
        })
        .insert(Executing)
        .id();

    // construct sacus
    let mut sacu_query = sim
        .world
        .query_filtered::<Entity, (With<RASSupportCommander>, With<Executing>)>();
    loop {
        sim.run();
        sim.print_tick();
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
        if sacu_count >= target_count {
            // run until target number of sacus
            break;
        }
    }

    // stop gate
    sim.world.entity_mut(gate).remove::<Executing>();
    // create paragon
    let paragon = sim
        .world
        .spawn()
        .insert(PARAGON_DAMAGE)
        .insert(Paragon)
        .id();
    // construct paragon
    let sacus: Vec<Entity> = sacu_query.iter(&sim.world).collect();
    let sacu_count = sacus.len();
    for entity in sacus {
        sim.world.entity_mut(entity).insert(Constructing {
            target: paragon,
            build_amount: 0.0,
            energy_consumption_multiplier: 1.0,
            energy_requested: 0.0,
            mass_consumption_multiplier: 1.0,
            mass_requested: 0.0,
        });
    }

    let sacrifice_portion = f64::min(
        RAS_SACU_DAMAGE.mass_total * RAS_SACU_SACRIFICE.mass_efficiency / PARAGON_DAMAGE.mass_total,
        RAS_SACU_DAMAGE.energy_total * RAS_SACU_SACRIFICE.energy_efficiency
            / PARAGON_DAMAGE.energy_total,
    );
    let sacrifice_point = 1.0 - sacu_count as f64 * sacrifice_portion;
    assert!(sacrifice_point < 1.0);
    // wait until close to sacrifice point
    loop {
        sim.run();
        sim.print_tick();
        sim.print_economy();
        if let Some(damage) = sim.world.entity(paragon).get::<Damage>() {
            println!("  Paragon build progress: {:.2}%", damage.health * 100.0);
            if damage.health >= sacrifice_point {
                break;
            }
        }
    }

    let mut sacu_res_query = sim
        .world
        .query_filtered::<&ResourceProducer, (With<RASSupportCommander>, With<Executing>)>();
    println!("SACU resource production totals");
    let mut mass_total = 0.0;
    let mut energy_total = 0.0;
    for res in sacu_res_query.iter(&sim.world) {
        mass_total += res.total_mass;
        energy_total += res.total_energy;
        println!("  mass: {:.2}, energy: {:.2}", res.total_mass, res.total_energy);
    }
    println!("total mass: {:.2}", mass_total);
    println!("total energy: {:.2}", energy_total);

    // sacrifice sacus
    println!("Sacrificing");
    let sacus: Vec<Entity> = sacu_query.iter(&sim.world).collect();
    for entity in sacus {
        let mut handle = sim.world.entity_mut(entity);
        handle.remove::<Constructing>();
        handle.insert(Sacrificing { target: paragon });
    }

    sim.run();
    sim.print_tick();
    sim.print_economy();
    if let Some(damage) = sim.world.entity(paragon).get::<Damage>() {
        println!("  Paragon build progress: {:.2}%", damage.health * 100.0);
    }

    let tick = sim.get_tick();
    println!("Total time: {} minutes", tick as f64 / 10. / 60.);
    println!(
        "Time to build paragon directly: {} minutes",
        PARAGON_DAMAGE.mass_total / mass_yield / 60.
    );
}
