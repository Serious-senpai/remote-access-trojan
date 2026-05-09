use wdk_sys::{DEVICE_OBJECT, IO_STACK_LOCATION, IRP, IRP_MJ_CREATE, NTSTATUS};

use crate::handlers::irp::IrpHandler;

pub struct CreateHandler<'a> {
    _irp: &'a mut IRP,
}

impl<'a> IrpHandler<'a> for CreateHandler<'a> {
    const CODE: u32 = IRP_MJ_CREATE;

    fn new(
        _: &'a DEVICE_OBJECT,
        irp: &'a mut IRP,
        _: &'a mut IO_STACK_LOCATION,
    ) -> anyhow::Result<Self> {
        Ok(Self { _irp: irp })
    }

    fn handle(&mut self) -> anyhow::Result<(), NTSTATUS> {
        self._irp.IoStatus.Information = 0;
        Ok(())
    }
}
