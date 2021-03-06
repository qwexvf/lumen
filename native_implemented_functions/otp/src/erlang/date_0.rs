#[cfg(test)]
mod test;

use liblumen_alloc::erts::exception;
use liblumen_alloc::erts::process::Process;
use liblumen_alloc::erts::term::prelude::*;
use lumen_rt_core::time::datetime;

use native_implemented_function::native_implemented_function;

#[native_implemented_function(date/0)]
pub fn native(process: &Process) -> exception::Result<Term> {
    let date: [usize; 3] = datetime::local_date();

    process
        .tuple_from_slice(&[
            process.integer(date[0])?,
            process.integer(date[1])?,
            process.integer(date[2])?,
        ])
        .map_err(|error| error.into())
}
