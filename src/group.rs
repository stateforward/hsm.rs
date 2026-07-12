use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::context::Context;
use crate::element::{AttributeValue, Instance};
use crate::error::{HsmError, Result};
use crate::event::Event;
use crate::hsm_impl::HSM;
use crate::runtime::{
    AttributeGetTarget, AttributeSetTarget, DispatchTarget, OperationCallTarget, RestartDataTarget,
    RestartTarget, RuntimeIdentityTarget, Snapshot, SnapshotTarget, StartDataTarget, StopTarget,
};

#[derive(Clone)]
pub enum GroupMember<T: Instance> {
    Machine(HSM<T>),
    Group(Group<T>),
}

impl<T: Instance> From<HSM<T>> for GroupMember<T> {
    fn from(machine: HSM<T>) -> Self {
        Self::Machine(machine)
    }
}

impl<T: Instance> From<Group<T>> for GroupMember<T> {
    fn from(group: Group<T>) -> Self {
        Self::Group(group)
    }
}

#[derive(Clone)]
pub struct Group<T: Instance> {
    id: String,
    machines: Vec<HSM<T>>,
}

impl<T: Instance> Group<T> {
    pub fn new(machines: Vec<HSM<T>>) -> Self {
        Self::with_id("", machines)
    }

    pub fn with_id(id: impl Into<String>, machines: Vec<HSM<T>>) -> Self {
        Self {
            id: id.into(),
            machines,
        }
    }

    pub fn with_members(members: Vec<GroupMember<T>>) -> Self {
        Self::with_id_and_members("", members)
    }

    pub fn with_id_and_members(id: impl Into<String>, members: Vec<GroupMember<T>>) -> Self {
        let mut machines = Vec::new();
        for member in members {
            match member {
                GroupMember::Machine(machine) => machines.push(machine),
                GroupMember::Group(group) => machines.extend(group.machines),
            }
        }
        Self {
            id: id.into(),
            machines,
        }
    }

    pub fn machines(&self) -> Vec<HSM<T>> {
        self.machines.clone()
    }

    pub fn len(&self) -> usize {
        self.machines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.machines.is_empty()
    }

    pub fn context(&self) -> Context {
        self.machines
            .first()
            .map(|machine| machine.context().clone())
            .unwrap_or_default()
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[allow(non_snake_case)]
    pub fn ID(&self) -> String {
        self.id()
    }

    pub fn name(&self) -> String {
        self.id()
    }

    #[allow(non_snake_case)]
    pub fn Name(&self) -> String {
        self.name()
    }

    pub fn qualified_name(&self) -> String {
        self.id()
    }

    #[allow(non_snake_case)]
    pub fn QualifiedName(&self) -> String {
        self.qualified_name()
    }

    pub fn state(&self) -> Vec<String> {
        self.machines.iter().map(HSM::state).collect()
    }

    pub fn current_state(&self) -> Vec<String> {
        self.state()
    }

    pub fn get(&self, name: &str) -> Option<AttributeValue> {
        self.machines.first().and_then(|machine| machine.get(name))
    }

    #[allow(non_snake_case)]
    pub fn Get(&self, name: &str) -> Option<AttributeValue> {
        self.get(name)
    }

    pub fn set<V: Into<AttributeValue>>(&self, name: &str, value: V) -> Result<()> {
        let value = value.into();
        for machine in &self.machines {
            if machine.is_started() {
                machine.set(name, value.clone())?;
            }
        }
        Ok(())
    }

    #[allow(non_snake_case)]
    pub fn Set<V: Into<AttributeValue>>(&self, name: &str, value: V) -> Result<()> {
        self.set(name, value)
    }

    pub fn call(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, Vec::new())
    }

    pub fn call_with_args(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<std::sync::Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let Some(machine) = self.machines.first().cloned() else {
            return Box::pin(async {
                Err(HsmError::Runtime(
                    "operation requires a started HSM".to_string(),
                ))
            });
        };
        machine.call_with_args(ctx, name, args)
    }

    #[allow(non_snake_case)]
    pub fn Call(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call(ctx, name)
    }

    #[allow(non_snake_case)]
    pub fn CallWithArgs(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<std::sync::Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, args)
    }

    pub fn dispatch(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let ctx = ctx.clone();
        let machines = self.machines.clone();
        Box::pin(async move {
            for machine in machines {
                if machine.is_started() {
                    machine.dispatch(&ctx, event.clone()).await?;
                }
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn Dispatch(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.dispatch(ctx, event)
    }

    pub fn start(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let machines = self.machines.clone();
        Box::pin(async move {
            if machines.iter().any(HSM::is_started) {
                return Err(HsmError::Validation("already started HSM".to_string()));
            }
            for machine in machines {
                machine.start().await?;
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn Start(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.start()
    }

    pub fn start_with_data<D>(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Clone + Send + Sync + 'static,
    {
        let machines = self.machines.clone();
        Box::pin(async move {
            if machines.iter().any(HSM::is_started) {
                return Err(HsmError::Validation("already started HSM".to_string()));
            }
            for machine in machines {
                machine.start_with_data(data.clone()).await?;
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn StartWithData<D>(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Clone + Send + Sync + 'static,
    {
        self.start_with_data(data)
    }

    pub fn stop(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let ctx = ctx.clone();
        let machines = self.machines.clone();
        Box::pin(async move {
            for machine in machines {
                machine.stop(&ctx).await?;
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn Stop(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.stop(ctx)
    }

    pub fn restart(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        let ctx = ctx.clone();
        let machines = self.machines.clone();
        Box::pin(async move {
            for machine in machines {
                machine.restart(&ctx).await?;
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn Restart(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart(ctx)
    }

    pub fn restart_with_data<D>(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Clone + Send + Sync + 'static,
    {
        let ctx = ctx.clone();
        let machines = self.machines.clone();
        Box::pin(async move {
            for machine in machines {
                machine.restart_with_data(&ctx, data.clone()).await?;
            }
            Ok(())
        })
    }

    #[allow(non_snake_case)]
    pub fn RestartWithData<D>(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
    where
        D: Any + Clone + Send + Sync + 'static,
    {
        self.restart_with_data(ctx, data)
    }

    pub fn take_snapshot(&self) -> Result<Vec<Snapshot>> {
        self.machines
            .iter()
            .map(HSM::take_snapshot)
            .collect::<Result<Vec<_>>>()
    }

    #[allow(non_snake_case)]
    pub fn TakeSnapshot(&self) -> Result<Vec<Snapshot>> {
        self.take_snapshot()
    }
}

impl<T: Instance> SnapshotTarget for Group<T> {
    type Snapshot = Vec<Snapshot>;

    fn take_snapshot_with_context(&self, _ctx: &Context) -> Result<Self::Snapshot> {
        self.take_snapshot()
    }
}

impl<T: Instance> DispatchTarget for Group<T> {
    fn dispatch_with_context(
        &self,
        ctx: &Context,
        event: Event,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.dispatch(ctx, event)
    }
}

impl<T: Instance> StopTarget for Group<T> {
    fn stop_with_context(&self, ctx: &Context) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.stop(ctx)
    }
}

impl<T: Instance> RestartTarget for Group<T> {
    fn restart_with_context(
        &self,
        ctx: &Context,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart(ctx)
    }
}

impl<T: Instance> AttributeGetTarget for Group<T> {
    fn get_attribute(&self, name: &str) -> Option<AttributeValue> {
        self.get(name)
    }
}

impl<T: Instance, V: Into<AttributeValue>> AttributeSetTarget<V> for Group<T> {
    fn set_attribute(&self, name: &str, value: V) -> Result<()> {
        self.set(name, value)
    }
}

impl<T: Instance> OperationCallTarget for Group<T> {
    fn call_operation(
        &self,
        ctx: &Context,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call(ctx, name)
    }

    fn call_operation_with_args(
        &self,
        ctx: &Context,
        name: &str,
        args: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.call_with_args(ctx, name, args)
    }
}

impl<T, D> StartDataTarget<D> for Group<T>
where
    T: Instance,
    D: Any + Clone + Send + Sync + 'static,
{
    fn start_with_data_target(&self, data: D) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.start_with_data(data)
    }
}

impl<T, D> RestartDataTarget<D> for Group<T>
where
    T: Instance,
    D: Any + Clone + Send + Sync + 'static,
{
    fn restart_with_data_with_context(
        &self,
        ctx: &Context,
        data: D,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        self.restart_with_data(ctx, data)
    }
}

impl<T: Instance> RuntimeIdentityTarget for Group<T> {
    fn runtime_id(&self) -> String {
        self.id()
    }

    fn runtime_name(&self) -> String {
        self.name()
    }

    fn runtime_qualified_name(&self) -> String {
        self.qualified_name()
    }
}
