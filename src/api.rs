use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::context::Context;
use crate::element::{AttributeValue, Instance};
use crate::error::Result;
use crate::event::Event;
use crate::group::Group;
use crate::hsm_impl::HSM;
use crate::model::Model;
use crate::runtime::{
    AttributeGetTarget, AttributeSetTarget, Clock, DispatchTarget, OperationCallTarget, QueueLenFn,
    QueuePopFn, QueuePushFn, RestartDataTarget, RestartTarget, RuntimeConfig,
    RuntimeIdentityTarget, RuntimeQueue, SleepFn, SnapshotTarget, StartDataTarget, StopTarget,
};

pub fn new<T: Instance + 'static>(instance: T, model: Model<T>) -> HSM<T> {
    HSM::new(instance, model)
}

#[allow(non_snake_case)]
pub fn New<T: Instance + 'static>(instance: T, model: Model<T>) -> HSM<T> {
    new(instance, model)
}

pub fn new_with_config<T: Instance + 'static>(
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> HSM<T> {
    HSM::new_with_config(instance, model, config)
}

#[allow(non_snake_case)]
pub fn NewWithConfig<T: Instance + 'static>(
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> HSM<T> {
    new_with_config(instance, model, config)
}

pub fn make_group<T: Instance + 'static>(machines: Vec<HSM<T>>) -> Group<T> {
    Group::new(machines)
}

#[allow(non_snake_case)]
pub fn MakeGroup<T: Instance + 'static>(machines: Vec<HSM<T>>) -> Group<T> {
    make_group(machines)
}

pub fn make_group_with_id<T: Instance + 'static>(id: &str, machines: Vec<HSM<T>>) -> Group<T> {
    Group::with_id(id, machines)
}

#[allow(non_snake_case)]
pub fn MakeGroupWithID<T: Instance + 'static>(id: &str, machines: Vec<HSM<T>>) -> Group<T> {
    make_group_with_id(id, machines)
}

#[allow(non_snake_case)]
pub fn NewGroup<T: Instance + 'static>(machines: Vec<HSM<T>>) -> Group<T> {
    make_group(machines)
}

pub fn start<T: Instance + 'static>(ctx: &Context, instance: T, model: Model<T>) -> Result<HSM<T>> {
    Ok(HSM::new_with_config_and_context(
        instance,
        model,
        RuntimeConfig::default(),
        ctx.clone(),
    ))
}

#[allow(non_snake_case)]
pub fn Start<T: Instance + 'static>(ctx: &Context, instance: T, model: Model<T>) -> Result<HSM<T>> {
    start(ctx, instance, model)
}

pub fn started<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        let machine = start(&ctx, instance, model)?;
        machine.start().await?;
        Ok(machine)
    })
}

#[allow(non_snake_case)]
pub fn Started<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>> {
    started(ctx, instance, model)
}

pub fn start_with_data<Target, D>(
    target: &Target,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    Target: StartDataTarget<D> + ?Sized,
{
    target.start_with_data_target(data)
}

#[allow(non_snake_case)]
pub fn StartWithData<Target, D>(
    target: &Target,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    Target: StartDataTarget<D> + ?Sized,
{
    start_with_data(target, data)
}

pub fn started_with_data<T, D>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>>
where
    T: Instance + 'static,
    D: Any + Send + Sync + 'static,
{
    let ctx = ctx.clone();
    Box::pin(async move {
        let machine = start(&ctx, instance, model)?;
        machine.start_with_data(data).await?;
        Ok(machine)
    })
}

#[allow(non_snake_case)]
pub fn StartedWithData<T, D>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>>
where
    T: Instance + 'static,
    D: Any + Send + Sync + 'static,
{
    started_with_data(ctx, instance, model, data)
}

pub fn start_with_config<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Result<HSM<T>> {
    Ok(HSM::new_with_config_and_context(
        instance,
        model,
        config,
        ctx.clone(),
    ))
}

#[allow(non_snake_case)]
pub fn StartWithConfig<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Result<HSM<T>> {
    start_with_config(ctx, instance, model, config)
}

pub fn started_with_config<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>> {
    let ctx = ctx.clone();
    Box::pin(async move {
        let machine = start_with_config(&ctx, instance, model, config)?;
        machine.start().await?;
        Ok(machine)
    })
}

#[allow(non_snake_case)]
pub fn StartedWithConfig<T: Instance + 'static>(
    ctx: &Context,
    instance: T,
    model: Model<T>,
    config: RuntimeConfig,
) -> Pin<Box<dyn Future<Output = Result<HSM<T>>> + Send>> {
    started_with_config(ctx, instance, model, config)
}

pub fn stop<Target: StopTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    target.stop_with_context(ctx)
}

#[allow(non_snake_case)]
pub fn Stop<Target: StopTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    stop(ctx, target)
}

pub fn restart<Target: RestartTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    target.restart_with_context(ctx)
}

#[allow(non_snake_case)]
pub fn Restart<Target: RestartTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    restart(ctx, target)
}

pub fn restart_with_data<Target, D>(
    ctx: &Context,
    target: &Target,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    Target: RestartDataTarget<D> + ?Sized,
{
    target.restart_with_data_with_context(ctx, data)
}

#[allow(non_snake_case)]
pub fn RestartWithData<Target, D>(
    ctx: &Context,
    target: &Target,
    data: D,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
where
    Target: RestartDataTarget<D> + ?Sized,
{
    restart_with_data(ctx, target, data)
}

pub fn dispatch<Target: DispatchTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    target.dispatch_with_context(ctx, event)
}

#[allow(non_snake_case)]
pub fn Dispatch<Target: DispatchTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    event: Event,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    dispatch(ctx, target, event)
}

pub fn get<Target: AttributeGetTarget + ?Sized>(
    _ctx: &Context,
    target: &Target,
    name: &str,
) -> Option<AttributeValue> {
    target.get_attribute(name)
}

#[allow(non_snake_case)]
pub fn Get<Target: AttributeGetTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    name: &str,
) -> Option<AttributeValue> {
    get(ctx, target, name)
}

pub fn set<Target, V>(_ctx: &Context, target: &Target, name: &str, value: V) -> Result<()>
where
    Target: AttributeSetTarget<V> + ?Sized,
{
    target.set_attribute(name, value)
}

#[allow(non_snake_case)]
pub fn Set<Target, V>(ctx: &Context, target: &Target, name: &str, value: V) -> Result<()>
where
    Target: AttributeSetTarget<V> + ?Sized,
{
    set(ctx, target, name, value)
}

#[allow(non_snake_case)]
pub fn Config() -> RuntimeConfig {
    RuntimeConfig::default()
}

pub fn clock(sleep: Option<SleepFn>) -> Clock {
    Clock { Sleep: sleep }.with_defaults()
}

#[allow(non_snake_case)]
pub fn Clock(sleep: Option<SleepFn>) -> Clock {
    clock(sleep)
}

pub fn queue(push: QueuePushFn, pop: QueuePopFn, len: QueueLenFn) -> RuntimeQueue {
    RuntimeQueue::new(push, pop, len)
}

#[allow(non_snake_case)]
pub fn Queue(push: QueuePushFn, pop: QueuePopFn, len: QueueLenFn) -> RuntimeQueue {
    queue(push, pop, len)
}

#[allow(non_snake_case)]
pub fn ID<Target: RuntimeIdentityTarget + ?Sized>(target: &Target) -> String {
    target.runtime_id()
}

#[allow(non_snake_case)]
pub fn Name<Target: RuntimeIdentityTarget + ?Sized>(target: &Target) -> String {
    target.runtime_name()
}

#[allow(non_snake_case)]
pub fn QualifiedName<Target: RuntimeIdentityTarget + ?Sized>(target: &Target) -> String {
    target.runtime_qualified_name()
}

#[allow(non_snake_case)]
pub fn Data<T: Instance + 'static>(machine: &HSM<T>) -> Option<Arc<dyn Any + Send + Sync>> {
    machine.data()
}

pub fn call<Target: OperationCallTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    name: &str,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    target.call_operation(ctx, name)
}

pub fn call_with_args<Target: OperationCallTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    name: &str,
    args: Vec<Arc<dyn Any + Send + Sync>>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    target.call_operation_with_args(ctx, name, args)
}

#[allow(non_snake_case)]
pub fn Call<Target: OperationCallTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    name: &str,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    call(ctx, target, name)
}

#[allow(non_snake_case)]
pub fn CallWithArgs<Target: OperationCallTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
    name: &str,
    args: Vec<Arc<dyn Any + Send + Sync>>,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
    call_with_args(ctx, target, name, args)
}

pub fn take_snapshot<Target: SnapshotTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Result<Target::Snapshot> {
    target.take_snapshot_with_context(ctx)
}

#[allow(non_snake_case)]
pub fn TakeSnapshot<Target: SnapshotTarget + ?Sized>(
    ctx: &Context,
    target: &Target,
) -> Result<Target::Snapshot> {
    take_snapshot(ctx, target)
}
