use bevy_ecs::prelude::*;

/// ticks per second
pub const TICK_RATE: f64 = 10.0;
/// smallest considered floating point value
pub const EPSILON: f64 = 1e-6;

/// FA resource economy
#[derive(Debug)]
pub struct Economy {
    /// current available mass
    pub mass: f64,
    /// current available energy
    pub energy: f64,
    /// total capacity for mass
    pub mass_capacity: f64,
    /// total capacity for energy
    pub energy_capacity: f64,
    /// mass stall ratio (0.5 means 2x as much mass requested as produced)
    pub mass_stall: f64,
    /// energy stall ratio
    pub energy_stall: f64,
    /// total mass production
    pub mass_produced: f64,
    /// total energy production
    pub energy_produced: f64,
    /// total mass requests
    pub mass_requested: f64,
    /// total energy requests
    pub energy_requested: f64,
    /// total mass consumed
    pub mass_consumed: f64,
    /// total energy consumed
    pub energy_consumed: f64,
}

impl Default for Economy {
    fn default() -> Self {
        Economy {
            mass: 0.0,
            energy: 0.0,
            mass_capacity: 4000.0,
            energy_capacity: 100000.0,
            mass_stall: 1.0,
            energy_stall: 1.0,
            mass_produced: 0.0,
            energy_produced: 0.0,
            mass_requested: 0.0,
            energy_requested: 0.0,
            mass_consumed: 0.0,
            energy_consumed: 0.0,
        }
    }
}

/// Tick counter
pub struct CurrentTick(pub u64);

/// System log handler
pub struct LogHandler {
    pub emit: Box<dyn Fn(String) + Send + Sync>,
}

impl LogHandler {
    pub fn new(handler: impl Fn(String) + Send + Sync + 'static) -> LogHandler {
        LogHandler {
            emit: Box::new(handler),
        }
    }
}

#[derive(Component)]
pub struct ConstructionPaused;

/// Indicates entities which are currently executing
#[derive(Component)]
pub struct Executing;

/// Indicates entities which will begin executing following construction
#[derive(Component)]
pub struct WillExecuteOnConstruct;

/// Entity produces resources
#[derive(Component)]
pub struct ResourceProducer {
    /// mass produced per tick
    pub mass_yield: f64,
    /// energy produced per tick
    pub energy_yield: f64,
    /// total mass produced
    pub total_mass: f64,
    /// total energy produced
    pub total_energy: f64,
}

impl Default for ResourceProducer {
    fn default() -> Self {
        ResourceProducer {
            mass_yield: 0.0,
            energy_yield: 0.0,
            total_mass: 0.0,
            total_energy: 0.0,
        }
    }
}

/// Entity consumes resources
/// TODO: refactor this: units declare resource consumption, stall ratio
/// calculated, then units pull resources as necessary instead of allocations
#[derive(Component)]
pub struct ResourceConsumer {
    /// how much mass the entity wants
    pub mass_request: f64,
    /// how much energy the entity wants
    pub energy_request: f64,
    /// how much mass the entity actually consumed
    pub mass_consumed: f64,
    /// how much  energy the entity actually consumed
    pub energy_consumed: f64,
}

impl Default for ResourceConsumer {
    fn default() -> Self {
        ResourceConsumer {
            mass_request: 0.0,
            energy_request: 0.0,
            mass_consumed: 0.0,
            energy_consumed: 0.0,
        }
    }
}

/// Entity can be damaged
#[derive(Component)]
pub struct Damage {
    /// health as a fraction (0.0 = dead, 1.0 = full health)
    pub health: f64,
    /// total health points of unit
    pub health_points: u64,
    /// total mass cost of unit
    pub mass_total: f64,
    /// total energy cost of unit
    pub energy_total: f64,
    /// build time, unitless (see build_rate)
    pub build_time: f64,
}

/// Entity has an engineering suite (can build stuff)
#[derive(Component)]
pub struct Engineering {
    /// how fast this unit can build (build_time per tick)
    pub build_rate: f64,
}

/// Entity is currently constructing another entity
#[derive(Component)]
pub struct Constructing {
    /// entity currently being constructed
    pub target: Entity,
    /// mass requested for construction
    pub mass_requested: f64,
    /// energy requested for construction
    pub energy_requested: f64,
    /// mass consumption multiplier (example: 0.9 if adjacency bonus)
    pub mass_consumption_multiplier: f64,
    /// energy consumption multiplier
    pub energy_consumption_multiplier: f64,
    /// proportion of unit that would be completed this tick by this unit if no stall
    pub build_amount: f64,
}

// systems
/// update tick counter
pub fn count_tick(mut tick_counter: ResMut<CurrentTick>) {
    tick_counter.0 += 1;
}

/// resource production accounting
pub fn economy_resource_producers(
    mut query: Query<&mut ResourceProducer, With<Executing>>,
    mut economy: ResMut<Economy>,
) {
    let mut total_mass = 0.0;
    let mut total_energy = 0.0;
    for mut producer in &mut query {
        total_mass += producer.mass_yield;
        total_energy += producer.energy_yield;
        producer.total_mass += producer.mass_yield;
        producer.total_energy += producer.energy_yield;
    }
    economy.mass += total_mass;
    economy.energy += total_energy;
    economy.mass_produced = total_mass;
    economy.energy_produced = total_energy;
}

pub fn economy_process_resource_requests(
    query: Query<&mut ResourceConsumer, With<Executing>>,
    mut economy: ResMut<Economy>,
) {
    let mut total_mass_requested = 0.0;
    let mut total_energy_requested = 0.0;
    for consumer in &query {
        total_mass_requested += consumer.mass_request;
        total_energy_requested += consumer.energy_request;
    }

    economy.mass_stall = f64::min(1.0, economy.mass / total_mass_requested);
    economy.energy_stall = f64::min(1.0, economy.energy / total_energy_requested);
    economy.mass_requested = total_mass_requested;
    economy.energy_requested = total_energy_requested;
}

pub fn economy_process_resource_consumption(
    mut query: Query<&mut ResourceConsumer, With<Executing>>,
    mut economy: ResMut<Economy>,
    current_tick: Res<CurrentTick>,
    log_handler: Res<LogHandler>,
) {
    let mut total_mass_consumed = 0.0;
    let mut total_energy_consumed = 0.0;
    for mut consumer in &mut query {
        total_mass_consumed += consumer.mass_consumed;
        total_energy_consumed += consumer.energy_consumed;
        consumer.mass_consumed = 0.0;
        consumer.energy_consumed = 0.0;
        consumer.mass_request = 0.0;
        consumer.energy_request = 0.0;
    }

    economy.mass = f64::min(economy.mass_capacity, economy.mass - total_mass_consumed);
    economy.energy = f64::min(
        economy.energy_capacity,
        economy.energy - total_energy_consumed,
    );
    economy.mass_consumed = total_mass_consumed;
    economy.energy_consumed = total_energy_consumed;

    if economy.mass < -1.0 || economy.energy < -1.0 {
        (log_handler.emit)(format!(
            "tick {}: warn: overconsumption, mass {} energy {}",
            current_tick.0, economy.mass, economy.energy
        ));
    }
}

pub fn execute_on_finished_construction(
    query: Query<
        (Entity, &Damage),
        (
            Changed<Damage>,
            Without<Executing>,
            With<WillExecuteOnConstruct>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, damage) in &query {
        if damage.health == 1.0 {
            commands.entity(entity).remove::<WillExecuteOnConstruct>();
            commands.entity(entity).insert(Executing);
        }
    }
}

pub fn do_construct_resources_request(
    mut construct_query: Query<
        (
            Entity,
            &mut Constructing,
            &Engineering,
            &mut ResourceConsumer,
        ),
        (With<Executing>, Without<ConstructionPaused>),
    >,
    mut target_query: Query<&mut Damage>,
    mut commands: Commands,
) {
    for (entity, mut constructing, engineering, mut resource_consumer) in &mut construct_query {
        if let Ok(target_damage) = target_query.get_mut(constructing.target) {
            let build_amount = engineering.build_rate / target_damage.build_time;
            constructing.build_amount = build_amount;
            constructing.mass_requested =
                build_amount * target_damage.mass_total * constructing.mass_consumption_multiplier;
            constructing.energy_requested = build_amount
                * target_damage.energy_total
                * constructing.energy_consumption_multiplier;
            resource_consumer.mass_request += constructing.mass_requested;
            resource_consumer.energy_request += constructing.energy_requested;
        } else {
            // target gone, remove constructing component
            commands.entity(entity).remove::<Constructing>();
        }
    }
}

pub fn do_construct(
    mut construct_query: Query<
        (Entity, &Constructing, &mut ResourceConsumer),
        (With<Executing>, Without<ConstructionPaused>),
    >,
    mut target_query: Query<&mut Damage>,
    mut commands: Commands,
    economy: Res<Economy>,
) {
    for (entity, constructing, mut resource_consumer) in &mut construct_query {
        if let Ok(mut target_damage) = target_query.get_mut(constructing.target) {
            // if target is done constructing, remove constructing component
            if target_damage.health >= 1.0 {
                // greater should never happen
                commands.entity(entity).remove::<Constructing>();
                continue;
            }
            // determine resource usage
            // resources available to use
            let mass_available = constructing.mass_requested * economy.mass_stall
                / constructing.mass_consumption_multiplier;
            let energy_available = constructing.energy_requested * economy.energy_stall
                / constructing.energy_consumption_multiplier;
            // determine resource bottleneck
            let min_portion = f64::min(
                mass_available / target_damage.mass_total,
                energy_available / target_damage.energy_total,
            );
            // calculate total used
            let mass_used =
                min_portion * target_damage.mass_total * constructing.mass_consumption_multiplier;
            let energy_used = min_portion
                * target_damage.energy_total
                * constructing.energy_consumption_multiplier;

            if target_damage.health + min_portion >= 1.0 {
                // allocation would overflow target total mass/energy cost
                let mass_remaining = (1.0 - target_damage.health) * target_damage.mass_total;
                let energy_remaining = (1.0 - target_damage.health) * target_damage.energy_total;
                resource_consumer.mass_consumed +=
                    mass_remaining * constructing.mass_consumption_multiplier;
                resource_consumer.energy_consumed +=
                    energy_remaining * constructing.energy_consumption_multiplier;
                // target is done
                target_damage.health = 1.0;
                commands.entity(entity).remove::<Constructing>();
            } else {
                // update resource consumption
                resource_consumer.mass_consumed += mass_used;
                resource_consumer.energy_consumed += energy_used;
                // apply construction progress
                target_damage.health += min_portion;
            }
        }
    }
}

pub struct FASimulation {
    pub world: World,
    pub update_schedule: Schedule,
}

impl FASimulation {
    pub fn new() -> Self {
        let mut world = World::new();

        // resources
        world.insert_resource(CurrentTick(0));
        world.insert_resource(Economy {
            mass_capacity: 4000.0,
            energy_capacity: 100000.0,
            ..Default::default()
        });
        world.insert_resource(LogHandler::new(|message| println!("{}", message)));

        // schedule and stages
        let mut schedule = Schedule::default();
        let tick_stage = SystemStage::single_threaded().with_system(count_tick);
        let update_stage = SystemStage::parallel()
            .with_system(execute_on_finished_construction)
            .with_system(do_construct_resources_request);
        let economy_request_stage = SystemStage::parallel()
            .with_system(economy_resource_producers)
            .with_system(economy_process_resource_requests.after(economy_resource_producers));
        let resource_usage_stage = SystemStage::parallel().with_system(do_construct);
        let economy_accounting_stage =
            SystemStage::parallel().with_system(economy_process_resource_consumption);

        schedule.add_stage("tick count", tick_stage);
        schedule.add_stage("update", update_stage);
        schedule.add_stage("economy request", economy_request_stage);
        schedule.add_stage("resource usage", resource_usage_stage);
        schedule.add_stage("economy accounting", economy_accounting_stage);

        FASimulation {
            world,
            update_schedule: schedule,
        }
    }

    pub fn run(&mut self) {
        self.update_schedule.run(&mut self.world);
    }
}
