use rust::*;
use std::env;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

fn get_memory_mb() -> f64 {
    0.0
}

#[derive(Debug)]
pub struct TrafficLight {
    pub maintenance_mode: bool,
    pub cars_waiting: i32,
    pub timer: i32,
}

impl TrafficLight {
    pub fn new() -> Self {
        Self {
            maintenance_mode: false,
            cars_waiting: 0,
            timer: 0,
        }
    }
}

impl Instance for TrafficLight {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn reset_cars(
    _ctx: &Context,
    inst: &mut TrafficLight,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.cars_waiting = 0;
    Box::pin(async move {})
}

fn add_car(
    _ctx: &Context,
    inst: &mut TrafficLight,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.cars_waiting += 1;
    Box::pin(async move {})
}

fn no_cars_waiting(_ctx: &Context, inst: &TrafficLight, _event: &Event) -> bool {
    inst.cars_waiting == 0
}

fn is_maintenance(_ctx: &Context, inst: &TrafficLight, _event: &Event) -> bool {
    inst.maintenance_mode
}

fn is_not_maintenance(_ctx: &Context, inst: &TrafficLight, _event: &Event) -> bool {
    !inst.maintenance_mode
}

fn check_cars_for_choice(_ctx: &Context, inst: &TrafficLight, _event: &Event) -> bool {
    inst.cars_waiting > 10
}

fn set_timer_extended(
    _ctx: &Context,
    inst: &mut TrafficLight,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.timer = 60;
    Box::pin(async move {})
}

fn set_timer_standard(
    _ctx: &Context,
    inst: &mut TrafficLight,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.timer = 40;
    Box::pin(async move {})
}

fn maintenance_tick(
    _ctx: &Context,
    inst: &mut TrafficLight,
    _event: &Event,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    inst.timer += 1;
    Box::pin(async move {})
}

fn env_usize(name: &str, default_value: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

fn env_bool(name: &str) -> bool {
    env::var(name)
        .map(|value| value != "" && value != "0" && value != "false" && value != "False")
        .unwrap_or(false)
}

fn assert_traffic_light(
    sm: &HSM<TrafficLight>,
    state: &str,
    cars_waiting: i32,
    timer: i32,
    step: &str,
) {
    if sm.state() != state {
        panic!("{}: state {}, expected {}", step, sm.state(), state);
    }
    let inst = sm.instance().read().unwrap();
    if inst.cars_waiting != cars_waiting {
        panic!(
            "{}: cars_waiting {}, expected {}",
            step, inst.cars_waiting, cars_waiting
        );
    }
    if inst.timer != timer {
        panic!("{}: timer {}, expected {}", step, inst.timer, timer);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let warmup_ms = env_usize("HSM_BENCH_WARMUP_MS", 250);
    let duration_ms_target = env_usize("HSM_BENCH_DURATION_MS", 2000);
    let ctx = Context::new();

    macro_rules! create_model {
        () => {
            define!(
                "TrafficLight",
                initial!(target!("operational")),
                state!(
                    "operational",
                    transition!(
                        on!("MaintenanceSwitch"),
                        guard!(is_maintenance),
                        target!("../maintenance")
                    ),
                    initial!(target!("red")),
                    state!(
                        "red",
                        transition!(
                            on!("TimerEvent"),
                            guard!(check_cars_for_choice),
                            effect!(set_timer_extended),
                            target!("../green")
                        ),
                        transition!(
                            on!("TimerEvent"),
                            effect!(set_timer_standard),
                            target!("../green")
                        ),
                        transition!(on!("CarArrival"), effect!(add_car))
                    ),
                    state!(
                        "green",
                        transition!(on!("TimerEvent"), target!("../yellow")),
                        transition!(
                            on!("PedestrianButton"),
                            guard!(no_cars_waiting),
                            target!("../yellow")
                        )
                    ),
                    state!(
                        "yellow",
                        defer!("CarArrival"),
                        transition!(on!("TimerEvent"), target!("../red"))
                    )
                ),
                state!(
                    "maintenance",
                    entry!(reset_cars),
                    transition!(on!("Tick"), effect!(maintenance_tick)),
                    transition!(
                        on!("MaintenanceSwitch"),
                        guard!(is_not_maintenance),
                        target!("../operational")
                    )
                )
            )
        };
    }

    macro_rules! dispatch_batch {
        ($sm:expr, $cycles:expr, $ctx:expr, $car_arrival:expr, $timer_event:expr) => {{
            for _ in 0..$cycles {
                $sm.dispatch($ctx, $car_arrival.clone()).await.unwrap();
                $sm.dispatch($ctx, $timer_event.clone()).await.unwrap();
                $sm.dispatch($ctx, $timer_event.clone()).await.unwrap();
                $sm.dispatch($ctx, $timer_event.clone()).await.unwrap();
            }
        }};
    }

    let warmup_inst = TrafficLight::new();
    let warmup_sm = start(&ctx, warmup_inst, create_model!()).unwrap();
    warmup_sm.start().await.unwrap();

    let car_arrival = Event::new("CarArrival");
    let timer_event = Event::new("TimerEvent");

    if env_bool("HSM_BENCH_VALIDATE") {
        let validation_sm = start(&ctx, TrafficLight::new(), create_model!()).unwrap();
        validation_sm.start().await.unwrap();
        assert_traffic_light(
            &validation_sm,
            "/TrafficLight/operational/red",
            0,
            0,
            "initial",
        );

        validation_sm
            .dispatch(&ctx, car_arrival.clone())
            .await
            .unwrap();
        assert_traffic_light(
            &validation_sm,
            "/TrafficLight/operational/red",
            1,
            0,
            "after CarArrival",
        );

        validation_sm
            .dispatch(&ctx, timer_event.clone())
            .await
            .unwrap();
        assert_traffic_light(
            &validation_sm,
            "/TrafficLight/operational/green",
            1,
            40,
            "after first TimerEvent",
        );

        validation_sm
            .dispatch(&ctx, timer_event.clone())
            .await
            .unwrap();
        assert_traffic_light(
            &validation_sm,
            "/TrafficLight/operational/yellow",
            1,
            40,
            "after second TimerEvent",
        );

        validation_sm
            .dispatch(&ctx, timer_event.clone())
            .await
            .unwrap();
        assert_traffic_light(
            &validation_sm,
            "/TrafficLight/operational/red",
            1,
            40,
            "after third TimerEvent",
        );
    }

    let mut batch_cycles = 1usize;
    loop {
        let start_time = Instant::now();
        dispatch_batch!(warmup_sm, batch_cycles, &ctx, car_arrival, timer_event);
        if start_time.elapsed().as_millis() >= 10 || batch_cycles >= (1 << 20) {
            break;
        }
        batch_cycles *= 2;
    }
    let warmup_start = Instant::now();
    while warmup_start.elapsed().as_millis() < warmup_ms as u128 {
        dispatch_batch!(warmup_sm, batch_cycles, &ctx, car_arrival, timer_event);
    }

    let inst = TrafficLight::new();
    let sm = start(&ctx, inst, create_model!()).unwrap();
    sm.start().await.unwrap();

    let start_time = Instant::now();
    let mut completed_cycles = 0usize;
    while start_time.elapsed().as_millis() < duration_ms_target as u128 {
        dispatch_batch!(sm, batch_cycles, &ctx, car_arrival, timer_event);
        completed_cycles += batch_cycles;
    }
    let duration = start_time.elapsed();

    let duration_ms = duration.as_millis();
    let total_dispatches = completed_cycles * 4;
    let mut ops_per_sec = 0;
    if duration.as_secs_f64() > 0.0 {
        ops_per_sec = (total_dispatches as f64 / duration.as_secs_f64()) as usize;
    }

    let mem_mb = get_memory_mb();

    println!(
        "{{\"language\": \"Rust\", \"iterations\": {}, \"duration_ms\": {}, \"memory_mb\": {:.3}, \"throughput_ops_per_sec\": {}}}",
        total_dispatches, duration_ms, mem_mb, ops_per_sec
    );

    Ok(())
}
