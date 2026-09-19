//! The stack machine that runs a [`Module`].
//!
//! One value stack shared by every frame: a frame's locals are the stack slots from its
//! `base` up, parameters first. Output is a stack of buffers so that `Capture` can
//! redirect a block's output into a value (`lem-let` blocks, slots, `lem-fn` bodies).
//! Iterators live on their own stack, unwound with the frame that opened them.
//!
//! Every error leaves through [`Vm::fail`], which looks the failing instruction's span up
//! in the function's parallel span table — so a runtime error cannot exist without one.
//!
//! A builtin that takes a function (`map`, `filter`) runs it through [`Machine::invoke`]:
//! the VM pushes the call's frame and runs the loop again, nested, until that frame has
//! returned, then hands the value back to the builtin mid-instruction. An error inside
//! the invoked function is placed there, at its own instruction, and passes through the
//! builtin untouched as a [`Fault::Placed`].
//!
//! [`entry`] holds the three ways a caller starts one of these machines, and
//! [`operators`] what an operator means for the pair of values [`Vm::step`] hands it.

mod entry;
mod operators;

use std::collections::BTreeMap;
use std::rc::Rc;

use lemmon_syntax::Span;

use crate::builtins::{self, BUILTINS, Fault, Machine, ReceiverKind};
use crate::bytecode::{Function, MarkKey, MarkPayload, Module, Op};
use crate::rendered::{Buffer, Fragment, Fragments, Mark, Marked, Payload, Rendered};
use crate::value::{Closure, Map};
use crate::{RuntimeError, Value};

pub(crate) use entry::invoke;
pub use entry::{render, render_page};
use operators::{binary, index_into};

/// Frames a render may nest before it is an error. Recursion is legal in the language,
/// and a frame costs heap rather than stack, so this is generous: it is what stops a
/// template calling itself forever, not a stack budget. [`MAX_NATIVE_DEPTH`] is the
/// stack budget.
const MAX_DEPTH: usize = 256;

/// How deep a builtin may re-enter the VM before the recursion is refused.
///
/// [`MAX_DEPTH`] counts frames, which cost heap; this counts the times an entry like
/// `map` has called the VM's loop from inside its own `CallBuiltin` step, which costs
/// the machine's own stack — `step` → the entry → [`Machine::invoke`] → `run_until` →
/// `step` again (∆987). One cap cannot do both jobs: a page nesting components deeply
/// wants a generous frame count, while the `.wasm` has a megabyte of native stack,
/// pinned in `crates/lemmon-wasm/build.rs`, with no way to ask for more and no
/// diagnostic if it runs out.
///
/// Sized against that megabyte: a level costs about a kilobyte and a half compiled for
/// release and eight in a debug build, so sixty-four levels leaves the debug build —
/// the expensive one, and the one the tests run — using half of it.
const MAX_NATIVE_DEPTH: usize = 64;

struct Frame {
    func: u32,
    /// The next instruction to run.
    pc: usize,
    /// Where this frame's locals start on the value stack.
    base: usize,
    /// How many iterators were open when the frame was entered.
    iter_base: usize,
}

enum Iter {
    List(Rc<[Value]>, usize),
    Map(Rc<Map>, usize),
}

struct Vm<'a> {
    module: &'a Module,
    globals: &'a [(&'a str, Value)],
    stack: Vec<Value>,
    frames: Vec<Frame>,
    /// How many builtins are part-way through a call of their own, each holding a
    /// native stack frame this VM is running inside of.
    native_depth: usize,
    /// How many ids `uniqueId` has given out under each prefix. A `BTreeMap` and not a
    /// hash map: nothing here iterates it, but a deterministic order costs nothing and
    /// is one less thing to have to argue about when a render has to be reproducible
    /// (invariant 1). Empty at the start of every render, which is what makes two
    /// renders of one page agree.
    ids: BTreeMap<Rc<str>, u64>,
    iters: Vec<Iter>,
    out: Vec<Buffer>,
    /// What collect blocks have gathered: `Some` in a page render, and `None` while the
    /// loader runs a function, which renders no page to collect for.
    fragments: Option<Fragments>,
}

impl<'a> Vm<'a> {
    /// A machine about to render: nothing on any stack, a root output buffer, and
    /// somewhere for collect blocks to gather.
    fn new(module: &'a Module, globals: &'a [(&'a str, Value)]) -> Self {
        Self {
            module,
            globals,
            stack: Vec::new(),
            frames: Vec::new(),
            native_depth: 0,
            ids: BTreeMap::new(),
            iters: Vec::new(),
            out: vec![Buffer::default()],
            fragments: Some(Fragments::default()),
        }
    }

    /// Runs the file entry `func` to its end and discards what it returned: a file's
    /// output is what it printed.
    fn run_entry(&mut self, func: u32) -> Result<(), RuntimeError> {
        self.call(func, 0, &[]).map_err(|message| {
            // The entry takes no arguments, so the only way in here is the depth cap,
            // which cannot trip on the first frame.
            unreachable!("entry call failed: {message}")
        })?;
        self.run_until(0)?;
        self.pop();
        Ok(())
    }

    /// The render, once the root buffer holds the whole page.
    fn finish(mut self) -> Rendered {
        let (html, marks) = self.out.pop().expect("the root output buffer").into_parts();
        Rendered {
            html,
            marks,
            fragments: self.fragments.take().expect("a page render collects"),
        }
    }

    /// Runs until only `depth` frames remain: the whole render from `0`, or — inside
    /// [`Machine::invoke`] — until the frame it pushed has returned.
    fn run_until(&mut self, depth: usize) -> Result<(), RuntimeError> {
        while self.frames.len() > depth {
            let frame = self.frames.last_mut().expect("a running frame");
            let (func, pc) = (frame.func, frame.pc);
            frame.pc += 1;
            let op = self.function(func).code[pc];
            self.step(op).map_err(|fault| match fault {
                Fault::Message(message) => self.fail(func, pc, message),
                Fault::Placed(error) => error,
            })?;
        }
        Ok(())
    }

    fn function(&self, index: u32) -> &'a Function {
        let module: &'a Module = self.module;
        &module.functions[index as usize]
    }

    /// The file and span of the instruction running now, for what a render records
    /// rather than fails with. Read before anything re-enters the loop.
    fn here(&self) -> (u32, Span) {
        let frame = self.frames.last().expect("a running frame");
        let function = self.function(frame.func);
        (function.file, function.spans[frame.pc - 1])
    }

    fn fail(&self, func: u32, pc: usize, message: String) -> RuntimeError {
        let function = self.function(func);
        RuntimeError::new(
            self.module.files[function.file as usize].clone(),
            function.spans[pc],
            message,
        )
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("the compiler balances the stack")
    }

    fn constant(&self, index: u32) -> &'a Value {
        let module: &'a Module = self.module;
        &module.constants[index as usize]
    }

    fn constant_str(&self, index: u32) -> &'a str {
        self.constant_name(index)
    }

    /// The interned name at `index`, shared rather than copied: a mark carries its name
    /// as long as the page does.
    fn constant_name(&self, index: u32) -> &'a Rc<str> {
        match self.constant(index) {
            Value::Str(s) => s,
            other => unreachable!("the compiler interns names as strings, not {other:?}"),
        }
    }

    fn local(&self, slot: u16) -> usize {
        self.frames.last().expect("a running frame").base + slot as usize
    }

    fn jump(&mut self, to: u32) {
        self.frames.last_mut().expect("a running frame").pc = to as usize;
    }

    fn call(&mut self, func: u32, argc: u16, captures: &[Value]) -> Result<(), String> {
        let function = self.function(func);
        if function.params.len() != argc as usize {
            return Err(format!(
                "`{}` takes {} argument{}, got {argc}",
                function.name,
                function.params.len(),
                if function.params.len() == 1 { "" } else { "s" },
            ));
        }
        if self.frames.len() >= MAX_DEPTH {
            return Err(format!("call depth exceeded {MAX_DEPTH} frames"));
        }
        let base = self.stack.len() - argc as usize;
        self.stack
            .resize(base + function.n_locals as usize, Value::Null);
        for (value, &slot) in captures.iter().zip(&function.captures) {
            self.stack[base + slot as usize] = value.clone();
        }
        self.frames.push(Frame {
            func,
            pc: 0,
            base,
            iter_base: self.iters.len(),
        });
        Ok(())
    }

    fn call_value(&mut self, callee: Value, argc: u16) -> Result<(), String> {
        match callee {
            Value::Fn(closure) => self.call(closure.func, argc, &closure.captures),
            other => Err(format!("cannot call a {}", other.type_name())),
        }
    }

    /// `x.f(…)` where the checker knew nothing about `x`: what the typed path would
    /// have done for the value that turned up. A record's field `f` is called as the
    /// value it holds — a record has no methods; anything else takes the method `f` of
    /// its runtime type — a map's entries are never fields to the checker, so a map
    /// whose key spells a method's name resolves the same way typed and through `any`.
    fn call_dynamic(&mut self, name: u32, argc: u16) -> Result<(), Fault> {
        let name = self.constant_str(name);
        let at = self.stack.len() - 1 - argc as usize;
        let receiver = &self.stack[at];
        if let Value::Record(record) = receiver {
            let callee =
                record.fields.get(name).cloned().ok_or_else(|| {
                    format!("no field `{name}` on {}", receiver.type_name())
                })?;
            self.stack.remove(at);
            return Ok(self.call_value(callee, argc)?);
        }
        let found = ReceiverKind::of_value(receiver)
            .and_then(|kind| builtins::method_id(kind, name));
        let Some(id) = found else {
            return Err(
                format!("no method `{name}` on {}", self.stack[at].type_name()).into(),
            );
        };
        let builtin = &BUILTINS[id as usize];
        if builtin.arity != argc as usize + 1 {
            let wanted = builtin.written_arity();
            return Err(format!(
                "`.{name}()` takes {wanted} argument{}, got {argc}",
                if wanted == 1 { "" } else { "s" }
            )
            .into());
        }
        let args = self.stack.split_off(at);
        let result = (builtin.call)(self, &args)?;
        self.stack.push(result);
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per op is the readable shape"
    )]
    fn step(&mut self, op: Op) -> Result<(), Fault> {
        match op {
            Op::Const { index } => self.stack.push(self.constant(index).clone()),
            Op::Local { slot } => self.stack.push(self.stack[self.local(slot)].clone()),
            Op::StoreLocal { slot } => {
                let at = self.local(slot);
                self.stack[at] = self.pop();
            }
            Op::Global { func } => self.stack.push(Value::Fn(Rc::new(Closure {
                func,
                captures: Vec::new(),
            }))),
            Op::Ambient { name } => {
                let name = self.constant_str(name);
                let value = self
                    .globals
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| format!("`{name}` is not bound"))?;
                self.stack.push(value);
            }
            Op::GetField { name } => {
                let object = self.pop();
                let value = self.field(&object, name)?;
                self.stack.push(value);
            }
            Op::OptField { name } => {
                let object = self.pop();
                let value = match object {
                    Value::Null => Value::Null,
                    object => self.field(&object, name)?,
                };
                self.stack.push(value);
            }
            Op::Index => {
                let index = self.pop();
                let object = self.pop();
                self.stack.push(index_into(&object, &index)?);
            }
            Op::Call { func, argc } => self.call(func, argc, &[])?,
            Op::CallValue { argc } => {
                let callee = self.stack.remove(self.stack.len() - 1 - argc as usize);
                self.call_value(callee, argc)?;
            }
            Op::CallBuiltin { id, argc } => {
                let builtin = &BUILTINS[id as usize];
                debug_assert_eq!(builtin.arity, argc as usize);
                let args = self.stack.split_off(self.stack.len() - argc as usize);
                let result = (builtin.call)(self, &args)?;
                self.stack.push(result);
            }
            Op::CallDynamic { name, argc } => self.call_dynamic(name, argc)?,
            Op::MakeClosure { func, captures } => {
                let captures = self.stack.split_off(self.stack.len() - captures as usize);
                self.stack
                    .push(Value::Fn(Rc::new(Closure { func, captures })));
            }
            Op::MakeList { len } => {
                let items = self.stack.split_off(self.stack.len() - len as usize);
                self.stack.push(Value::list(items));
            }
            Op::MakeMap { len } => {
                let map = self.pop_pairs(len);
                self.stack.push(Value::map(map));
            }
            Op::MakeRecord { name, len } => {
                let fields = self.pop_pairs(len);
                let ty = self.constant_str(name);
                self.stack.push(Value::record(ty, fields));
            }
            Op::TypeTest { name } => {
                let name = self.constant_str(name);
                let value = self.pop();
                let holds = match &value {
                    Value::Record(record) => self.module.is_a(&record.name, name),
                    Value::Null => false,
                    other => {
                        return Err(format!(
                            "cannot test whether a {} is `{name}`: only a record carries \
                             its type",
                            other.type_name()
                        )
                        .into());
                    }
                };
                self.stack.push(Value::Bool(holds));
            }
            Op::Jump { to } => self.jump(to),
            Op::JumpIfFalse { to } => {
                if !self.pop().as_condition() {
                    self.jump(to);
                }
            }
            Op::JumpUnlessNull { to } => {
                if matches!(self.stack.last(), Some(Value::Null)) {
                    self.pop();
                } else {
                    self.jump(to);
                }
            }
            Op::JumpIfFalseKeep { to } => {
                if self.logical("&&")? {
                    self.pop();
                } else {
                    self.jump(to);
                }
            }
            Op::JumpIfTrueKeep { to } => {
                if self.logical("||")? {
                    self.jump(to);
                } else {
                    self.pop();
                }
            }
            Op::IterInit { exit } => {
                let (iter, empty) = match self.pop() {
                    Value::List(items) => {
                        let empty = items.is_empty();
                        (Iter::List(items, 0), empty)
                    }
                    Value::Map(map) => {
                        let empty = map.is_empty();
                        (Iter::Map(map, 0), empty)
                    }
                    other => {
                        return Err(
                            format!("cannot iterate over a {}", other.type_name()).into()
                        );
                    }
                };
                if empty {
                    self.jump(exit);
                } else {
                    self.iters.push(iter);
                }
            }
            Op::IterNext { exit, pair } => {
                let next = match self.iters.last_mut().expect("an open iterator") {
                    Iter::List(items, i) => items.get(*i).map(|item| {
                        let index = i64::try_from(*i).expect("list index fits in i64");
                        *i += 1;
                        (item.clone(), Value::Int(index))
                    }),
                    Iter::Map(map, i) => map.entry(*i).map(|(key, value)| {
                        *i += 1;
                        (Value::Str(key.clone()), value.clone())
                    }),
                };
                match next {
                    Some((first, second)) => {
                        self.stack.push(first);
                        if pair {
                            self.stack.push(second);
                        }
                    }
                    None => {
                        self.iters.pop();
                        self.jump(exit);
                    }
                }
            }
            Op::Neg => {
                let value = match self.pop() {
                    Value::Int(i) => Value::Int(i.checked_neg().ok_or("integer overflow")?),
                    Value::Float(f) => Value::Float(-f),
                    other => {
                        return Err(format!("cannot negate a {}", other.type_name()).into());
                    }
                };
                self.stack.push(value);
            }
            Op::Not => {
                let value = match self.pop() {
                    Value::Bool(b) => Value::Bool(!b),
                    other => {
                        return Err(
                            format!("`!` needs a bool, got {}", other.type_name()).into()
                        );
                    }
                };
                self.stack.push(value);
            }
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::Eq
            | Op::Ne
            | Op::Lt
            | Op::Le
            | Op::Gt
            | Op::Ge
            | Op::In => {
                let rhs = self.pop();
                let lhs = self.pop();
                self.stack.push(binary(op, &lhs, &rhs)?);
            }
            Op::Emit => {
                let value = self.pop();
                value.emit(self.out.last_mut().expect("an output buffer"))?;
            }
            Op::EmitText { index } => {
                let text = self.constant_str(index);
                self.out
                    .last_mut()
                    .expect("an output buffer")
                    .push_str(text);
            }
            Op::Capture => self.out.push(Buffer::default()),
            Op::EndCapture => {
                let captured = self.out.pop().expect("a capture buffer");
                self.stack.push(captured.into_value());
            }
            Op::Mark {
                name,
                key,
                payload: carried,
            } => {
                // The default is on top and a popped payload beneath it, which is the
                // order the compiler lowered them in.
                let text = match self.pop() {
                    Value::Html(text) => text,
                    Value::Marked(_) => {
                        return Err(
                            "a mark cannot stand inside another mark's default".into()
                        );
                    }
                    // Through `any`: the checker asks for `html` at the call.
                    other => {
                        return Err(format!(
                            "a mark's default is html, got {}",
                            other.type_name()
                        )
                        .into());
                    }
                };
                let value = match carried {
                    MarkPayload::Popped => Some(self.pop()),
                    MarkPayload::None | MarkPayload::Reported { .. } => None,
                };
                if let Some(payload) = &value {
                    if let Some(refusal) = payload.payload_refusal() {
                        return Err(refusal.into());
                    }
                    // Through `any`: the checker asks for a record at the call, and only
                    // where the host claimed nothing about the name.
                    if !matches!(payload, Value::Record(_) | Value::Map(_)) {
                        return Err(format!(
                            "a mark's payload is a record, got {}",
                            payload.type_name()
                        )
                        .into());
                    }
                }
                let key = match key {
                    MarkKey::None => None,
                    MarkKey::Const { index } => Some(self.constant_name(index).clone()),
                    // The call computed it, so it is read here. A string, as a written
                    // key is: a field holding anything else — markup, a number, null —
                    // leaves the mark unkeyed, which is what a written key that is not a
                    // literal string does too, so the two paths key alike.
                    MarkKey::Field { index } => {
                        let field = self.constant_str(index);
                        match value.as_ref().and_then(|payload| match payload {
                            Value::Record(record) => record.fields.get(field),
                            Value::Map(map) => map.get(field),
                            _ => None,
                        }) {
                            Some(Value::Str(key)) => Some(key.clone()),
                            _ => None,
                        }
                    }
                };
                let payload = match carried {
                    MarkPayload::None => None,
                    MarkPayload::Popped => value.map(Payload::Value),
                    MarkPayload::Reported { index } => {
                        Some(Payload::Reported(self.constant_name(index).clone()))
                    }
                };
                let name = self.constant_name(name).clone();
                let (file, span) = self.here();
                let end = u32::try_from(text.len())
                    .map_err(|_| "a mark's default is too long".to_owned())?;
                self.stack.push(Value::Marked(Rc::new(Marked {
                    text,
                    marks: Box::from([Mark {
                        name,
                        key,
                        payload,
                        start: 0,
                        end,
                        file,
                        span,
                    }]),
                })));
            }
            Op::Collect { name } => {
                let text = match self.pop() {
                    Value::Html(text) => text,
                    Value::Marked(_) => {
                        return Err("a mark cannot be collected into a fragment".into());
                    }
                    other => unreachable!("a collect block captures html, not {other:?}"),
                };
                let name = self.constant_str(name);
                let (file, span) = self.here();
                let Some(fragments) = &mut self.fragments else {
                    return Err("only a page render collects fragments, and this block \
                                ran while the data was loading"
                        .into());
                };
                fragments.push(name, Fragment { text, file, span });
            }
            Op::Return => {
                let value = self.pop();
                let frame = self.frames.pop().expect("a running frame");
                self.stack.truncate(frame.base);
                self.iters.truncate(frame.iter_base);
                self.stack.push(value);
            }
        }
        Ok(())
    }

    /// Pops `len` key/value pairs, pushed in order, as a map.
    fn pop_pairs(&mut self, len: u32) -> Map {
        let pairs = self.stack.split_off(self.stack.len() - 2 * len as usize);
        let mut entries = pairs.into_iter();
        let mut map = Vec::with_capacity(len as usize);
        while let (Some(key), Some(value)) = (entries.next(), entries.next()) {
            match key {
                Value::Str(key) => map.push((key, value)),
                other => unreachable!("the compiler interns keys, not {other:?}"),
            }
        }
        map.into_iter().collect()
    }

    fn field(&self, object: &Value, name: u32) -> Result<Value, String> {
        let name = self.constant_str(name);
        match object {
            Value::Map(map) => map
                .get(name)
                .cloned()
                .ok_or_else(|| format!("no field `{name}`")),
            Value::Record(record) => record
                .fields
                .get(name)
                .cloned()
                .ok_or_else(|| format!("no field `{name}` on {}", object.type_name())),
            Value::Null => Err(format!("cannot read field `{name}` of null")),
            other => Err(format!(
                "cannot read field `{name}` of a {}",
                other.type_name()
            )),
        }
    }

    /// The boolean on top of the stack, for a short-circuit operator.
    fn logical(&self, op: &str) -> Result<bool, String> {
        match self.stack.last() {
            Some(Value::Bool(b)) => Ok(*b),
            Some(other) => Err(format!("`{op}` needs a bool, got {}", other.type_name())),
            None => unreachable!("the compiler balances the stack"),
        }
    }
}

impl Machine for Vm<'_> {
    fn invoke(&mut self, callee: &Value, args: Vec<Value>) -> Result<Value, Fault> {
        let depth = self.frames.len();
        let base = self.stack.len();
        let argc = u16::try_from(args.len()).expect("a builtin passes fewer than 65536");
        self.stack.extend(args);
        if self.native_depth >= MAX_NATIVE_DEPTH {
            self.stack.truncate(base);
            return Err(format!("builtins nested more than {MAX_NATIVE_DEPTH} deep").into());
        }
        if let Err(message) = self.call_value(callee.clone(), argc) {
            self.stack.truncate(base);
            return Err(message.into());
        }
        self.native_depth += 1;
        let ran = self.run_until(depth);
        self.native_depth -= 1;
        ran.map_err(Fault::Placed)?;
        Ok(self.pop())
    }

    fn next_id(&mut self, prefix: &str) -> u64 {
        match self.ids.get_mut(prefix) {
            Some(next) => {
                *next += 1;
                *next - 1
            }
            None => {
                self.ids.insert(Rc::from(prefix), 1);
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lemmon_syntax::Span;

    use super::*;
    use crate::bytecode::Declared;
    use crate::decls::tests::record as declare;

    /// A function whose every instruction is spanned `10*pc..10*pc+5`, so a test can
    /// tell which instruction an error blamed.
    pub(super) fn function(
        name: &str,
        params: &[&str],
        captures: &[u16],
        code: Vec<Op>,
    ) -> Function {
        let spans = (0..code.len())
            .map(|pc| {
                let pc = u32::try_from(pc).unwrap();
                Span::new(pc * 10, pc * 10 + 5)
            })
            .collect();
        Function {
            name: name.to_owned(),
            params: params.iter().map(|p| Rc::from(*p)).collect(),
            captures: captures.to_vec(),
            file: 0,
            n_locals: 4,
            code,
            spans,
        }
    }

    /// The id of an ambient builtin, by name: a test names what it calls, and reordering
    /// the table cannot silently point one at a different entry.
    fn ambient(name: &str) -> u32 {
        builtins::lookup(name).expect("an ambient builtin")
    }

    /// The id of a method, by the kind that carries it.
    fn method_of(kind: ReceiverKind, name: &str) -> u32 {
        builtins::method_id(kind, name).expect("a method")
    }

    pub(super) fn module(constants: Vec<Value>, functions: Vec<Function>) -> Module {
        Module {
            files: vec!["t.lem".to_owned()],
            entries: vec![0],
            constants,
            functions,
            declarations: [
                declare("Article", None, &[]),
                declare("LinkPost", Some("Article"), &[]),
            ]
            .into_iter()
            .map(|decl| Declared { file: 0, decl })
            .collect(),
            ..Module::default()
        }
    }

    /// Runs `code` as the entry function with `constants`, returning the output.
    fn run(constants: Vec<Value>, code: Vec<Op>) -> Result<String, RuntimeError> {
        rendered(constants, code).map(|rendered| rendered.html)
    }

    /// As [`run`], returning the whole render.
    fn rendered(
        constants: Vec<Value>,
        mut code: Vec<Op>,
    ) -> Result<Rendered, RuntimeError> {
        code.extend([Op::Const { index: 0 }, Op::Return]);
        let mut constants = constants;
        constants.insert(0, Value::Null);
        let code = code
            .into_iter()
            .map(|op| match op {
                Op::EmitText { index } => Op::EmitText { index: index + 1 },
                Op::GetField { name } => Op::GetField { name: name + 1 },
                Op::OptField { name } => Op::OptField { name: name + 1 },
                Op::Ambient { name } => Op::Ambient { name: name + 1 },
                Op::CallDynamic { name, argc } => Op::CallDynamic {
                    name: name + 1,
                    argc,
                },
                Op::MakeRecord { name, len } => Op::MakeRecord {
                    name: name + 1,
                    len,
                },
                Op::TypeTest { name } => Op::TypeTest { name: name + 1 },
                Op::Collect { name } => Op::Collect { name: name + 1 },
                Op::Mark { name, key, payload } => Op::Mark {
                    name: name + 1,
                    key,
                    payload,
                },
                other => other,
            })
            .collect();
        render(
            &module(constants, vec![function("t.lem", &[], &[], code)]),
            "t.lem",
            &[("page", Value::str("the page"))],
        )
    }

    /// Shorthand: `Const { index }`, where index 0 is the null `run` prepends and
    /// index `n` is the caller's `constants[n - 1]`. Name-carrying ops (`EmitText`,
    /// `GetField`, …) are written 0-based and shifted by `run` instead.
    const fn k(index: u32) -> Op {
        Op::Const { index }
    }

    fn error<T: std::fmt::Debug>(result: Result<T, RuntimeError>) -> (u32, String) {
        let err = result.unwrap_err();
        assert_eq!(err.path, "t.lem");
        assert_eq!(err.span.end, err.span.start + 5);
        (err.span.start / 10, err.message)
    }

    #[test]
    fn emits_text_verbatim_and_values_by_type() {
        let out = run(
            vec![
                Value::str("<p>"),
                Value::str("a<b"),
                Value::html("<i>"),
                Value::Int(7),
            ],
            vec![
                Op::EmitText { index: 0 },
                k(2),
                Op::Emit,
                k(3),
                Op::Emit,
                k(4),
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "<p>a&lt;b<i>7");
    }

    #[test]
    fn blames_the_emitting_instruction_for_an_unprintable_value() {
        let (pc, message) = error(run(vec![], vec![k(0), Op::Emit]));
        assert_eq!((pc, message.as_str()), (1, "cannot print a null"));
    }

    #[test]
    fn captures_output_into_an_html_value() {
        let out = run(
            vec![Value::str("x<y")],
            vec![
                Op::Capture,
                k(1),
                Op::Emit,
                Op::EndCapture,
                Op::StoreLocal { slot: 0 },
                Op::EmitText { index: 0 },
                Op::Local { slot: 0 },
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "x<yx&lt;y");
    }

    #[test]
    fn calls_a_function_with_its_arguments_in_parameter_slots() {
        let callee = function(
            "f",
            &["a", "b"],
            &[],
            vec![
                Op::Local { slot: 1 },
                Op::Local { slot: 0 },
                Op::Sub,
                Op::Return,
            ],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::Const { index: 1 },
                Op::Call { func: 1, argc: 2 },
                Op::Emit,
                Op::Const { index: 0 },
                Op::Return,
            ],
        );
        let module = module(vec![Value::Int(1), Value::Int(10)], vec![entry, callee]);
        assert_eq!(render(&module, "t.lem", &[]).unwrap().html, "9");
    }

    #[test]
    fn rejects_a_call_with_the_wrong_number_of_arguments() {
        let callee = function("f", &["a"], &[], vec![Op::Return]);
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![
                Op::Global { func: 1 },
                Op::CallValue { argc: 0 },
                Op::Return,
            ],
        );
        let err = render(&module(vec![], vec![entry, callee]), "t.lem", &[]).unwrap_err();
        assert_eq!(err.message, "`f` takes 1 argument, got 0");
        assert_eq!(err.span, Span::new(10, 15));
    }

    #[test]
    fn pluralises_the_arity_message() {
        let callee = function("f", &["a", "b"], &[], vec![Op::Return]);
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![Op::Call { func: 1, argc: 0 }, Op::Return],
        );
        let err = render(&module(vec![], vec![entry, callee]), "t.lem", &[]).unwrap_err();
        assert_eq!(err.message, "`f` takes 2 arguments, got 0");
    }

    #[test]
    fn caps_the_call_depth() {
        let looping = function(
            "f",
            &[],
            &[],
            vec![Op::Call { func: 1, argc: 0 }, Op::Return],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![Op::Call { func: 1, argc: 0 }, Op::Return],
        );
        let err = render(&module(vec![], vec![entry, looping]), "t.lem", &[]).unwrap_err();
        assert_eq!(err.message, "call depth exceeded 256 frames");
    }

    #[test]
    fn refuses_to_call_a_value_that_is_not_a_function() {
        let (pc, message) = error(run(
            vec![Value::Int(1)],
            vec![k(1), Op::CallValue { argc: 0 }],
        ));
        assert_eq!((pc, message.as_str()), (1, "cannot call a int"));
    }

    #[test]
    fn lands_closure_captures_in_the_slots_the_function_names() {
        let lambda = function(
            "<lambda>",
            &["x"],
            &[2],
            vec![
                Op::Local { slot: 0 },
                Op::Local { slot: 2 },
                Op::Add,
                Op::Return,
            ],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::MakeClosure {
                    func: 1,
                    captures: 1,
                },
                Op::Const { index: 1 },
                Op::CallValue { argc: 1 },
                Op::Emit,
                Op::Const { index: 0 },
                Op::Return,
            ],
        );
        let module = module(vec![Value::Int(100), Value::Int(5)], vec![entry, lambda]);
        assert_eq!(render(&module, "t.lem", &[]).unwrap().html, "105");
    }

    #[test]
    fn unwinds_open_iterators_with_their_frame() {
        // `f` opens an iterator and returns mid-loop; the caller's loop must be intact.
        let f = function(
            "f",
            &[],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::IterInit { exit: 4 },
                Op::IterNext {
                    exit: 4,
                    pair: false,
                },
                Op::Return,
                Op::Const { index: 1 },
                Op::Return,
            ],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::IterInit { exit: 7 },
                Op::IterNext {
                    exit: 7,
                    pair: false,
                },
                Op::Emit,
                Op::Call { func: 1, argc: 0 },
                Op::Emit,
                Op::Jump { to: 2 },
                Op::Const { index: 1 },
                Op::Return,
            ],
        );
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let module = module(vec![list, Value::Null], vec![entry, f]);
        assert_eq!(render(&module, "t.lem", &[]).unwrap().html, "1121");
    }

    #[test]
    fn calls_a_builtin() {
        let out = run(
            vec![Value::str("<b>")],
            vec![
                k(1),
                Op::CallBuiltin {
                    id: ambient("raw"),
                    argc: 1,
                },
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "<b>");
    }

    #[test]
    fn reports_a_builtin_failure_at_the_call() {
        let (pc, message) = error(run(
            vec![Value::Int(1)],
            vec![
                k(1),
                Op::CallBuiltin {
                    id: ambient("raw"),
                    argc: 1,
                },
                Op::Emit,
            ],
        ));
        assert_eq!((pc, message.as_str()), (1, "raw() takes a string, got int"));
    }

    /// `map` (builtin 4) over `constants[0]` with `functions[1]` as the function, then
    /// `first` (builtin 2) on the result, emitted — with a value pushed before the call
    /// and emitted after it, so an unbalanced stack would show in the output.
    fn map_then_first(rest: Vec<Op>) -> Vec<Op> {
        let mut code = vec![
            Op::Const { index: 1 },
            Op::Const { index: 0 },
            Op::Global { func: 1 },
            Op::CallBuiltin {
                id: method_of(ReceiverKind::List, "map"),
                argc: 2,
            },
            Op::CallBuiltin {
                id: method_of(ReceiverKind::List, "first"),
                argc: 1,
            },
            Op::Emit,
            Op::Emit,
        ];
        code.extend(rest);
        code
    }

    #[test]
    fn invokes_a_function_from_a_builtin_and_hands_its_value_back() {
        let increment = function(
            "<lambda>",
            &["x"],
            &[],
            vec![
                Op::Local { slot: 0 },
                Op::Const { index: 2 },
                Op::Add,
                Op::Return,
            ],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            map_then_first(vec![Op::Const { index: 3 }, Op::Return]),
        );
        let constants = vec![
            Value::list(vec![Value::Int(41), Value::Int(1)]),
            Value::str("|"),
            Value::Int(1),
            Value::Null,
        ];
        let module = module(constants, vec![entry, increment]);
        assert_eq!(render(&module, "t.lem", &[]).unwrap().html, "42|");
    }

    #[test]
    fn places_an_error_inside_an_invoked_function_at_its_own_instruction() {
        let negate = function(
            "<lambda>",
            &["x"],
            &[],
            vec![Op::Local { slot: 0 }, Op::Neg, Op::Return],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            map_then_first(vec![Op::Const { index: 2 }, Op::Return]),
        );
        let constants = vec![
            Value::list(vec![Value::str("s")]),
            Value::str("|"),
            Value::Null,
        ];
        let err =
            render(&module(constants, vec![entry, negate]), "t.lem", &[]).unwrap_err();
        // The lambda's `Neg` is at pc 1; the entry's `CallBuiltin` is at pc 3.
        assert_eq!(
            (err.span.start, err.message.as_str()),
            (10, "cannot negate a string")
        );
    }

    #[test]
    fn blames_the_builtins_instruction_when_its_function_cannot_be_called() {
        let two_params = function(
            "<lambda>",
            &["a", "b"],
            &[],
            vec![Op::Local { slot: 0 }, Op::Return],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            map_then_first(vec![Op::Const { index: 2 }, Op::Return]),
        );
        let constants = vec![
            Value::list(vec![Value::Int(1)]),
            Value::str("|"),
            Value::Null,
        ];
        let err =
            render(&module(constants, vec![entry, two_params]), "t.lem", &[]).unwrap_err();
        assert_eq!(
            (err.span.start, err.message.as_str()),
            (30, "`<lambda>` takes 2 arguments, got 1")
        );
    }

    #[test]
    fn refuses_to_invoke_a_value_that_is_not_a_function() {
        let (pc, message) = error(run(
            vec![Value::list(vec![Value::Int(1)]), Value::Int(3)],
            vec![
                k(1),
                k(2),
                Op::CallBuiltin {
                    id: method_of(ReceiverKind::List, "map"),
                    argc: 2,
                },
                Op::Emit,
            ],
        ));
        assert_eq!((pc, message.as_str()), (2, "cannot call a int"));
    }

    #[test]
    fn caps_the_call_depth_through_nested_invokes() {
        let err = render(&nesting_module(), "t.lem", &[]).unwrap_err();
        assert_eq!(err.message, "builtins nested more than 64 deep");
        assert_eq!(err.span.start, 20);
    }

    #[test]
    fn counts_ids_from_zero_again_at_every_render() {
        // What makes `uniqueId` safe to have at all: the VM remembers between calls,
        // and forgets between renders, so a page rendered twice is byte-identical
        // (invariant 1). Two runs of one program, not two calls in one.
        let program = || {
            let id = ambient("uniqueId");
            // Index 1: `run` puts the null it returns at 0 and shifts the rest along.
            run(
                vec![Value::str("desc")],
                vec![
                    Op::Const { index: 1 },
                    Op::CallBuiltin { id, argc: 1 },
                    Op::Emit,
                    Op::Const { index: 1 },
                    Op::CallBuiltin { id, argc: 1 },
                    Op::Emit,
                ],
            )
        };
        assert_eq!(program().unwrap(), "desc-0desc-1");
        assert_eq!(program().unwrap(), "desc-0desc-1");
    }

    #[test]
    fn collects_a_captured_body_at_the_span_of_the_collect_that_first_gave_it() {
        // Constants, 0-based and shifted by `run`: the body's text, then the name.
        let body = [
            Op::Capture,
            Op::EmitText { index: 0 },
            Op::EndCapture,
            Op::Collect { name: 1 },
        ];
        let rendered = rendered(
            vec![Value::str("a {}"), Value::str("styles")],
            [body, body].concat(),
        )
        .unwrap();
        assert_eq!(rendered.html, "");
        let fragments = rendered.fragments.get("styles");
        assert_eq!(fragments.len(), 1);
        assert_eq!(&*fragments[0].text, "a {}");
        // The first `Collect` is the fourth instruction: the one that ran, not the one
        // after it.
        assert_eq!(
            (fragments[0].file, fragments[0].span),
            (0, Span::new(30, 35))
        );
    }

    /// The ops `mark(name)` with no default lowers to: the empty default, then the op.
    /// Written 0-based like every name-carrying op here, and shifted by `run`.
    const fn mark(default: u32, name: u32) -> [Op; 2] {
        [
            Op::Const { index: default + 1 },
            Op::Mark {
                name,
                key: MarkKey::None,
                payload: MarkPayload::None,
            },
        ]
    }

    #[test]
    fn places_a_captured_mark_wherever_the_capture_is_printed() {
        // Constants, 0-based: the default, the name, the capture's text, a separator.
        let constants = vec![
            Value::html(""),
            Value::str("styles"),
            Value::str("ab"),
            Value::str("-"),
        ];
        let [default, name] = mark(0, 1);
        // A mark reaches a buffer by being printed, which is what `<lem.mark(…)>` does.
        let capture = vec![
            Op::Capture,
            Op::EmitText { index: 2 },
            default,
            name,
            Op::Emit,
            Op::EndCapture,
            Op::StoreLocal { slot: 0 },
        ];
        let print = vec![Op::Local { slot: 0 }, Op::Emit];
        let twice = rendered(
            constants.clone(),
            [
                capture.clone(),
                print.clone(),
                vec![Op::EmitText { index: 3 }],
                print,
            ]
            .concat(),
        )
        .unwrap();
        assert_eq!(twice.html, "ab-ab");
        let ranges: Vec<(u32, u32)> =
            twice.marks.iter().map(|m| (m.start, m.end)).collect();
        assert_eq!(ranges, [(2, 2), (5, 5)]);
        // Both come from the one call, and point at it: the `Mark` op is the fourth
        // instruction of the capture above.
        assert!(twice.marks.iter().all(|mark| {
            (&*mark.name, mark.file, mark.span) == ("styles", 0, Span::new(30, 35))
        }));
        // A capture never printed places nothing.
        let unprinted = rendered(constants, capture).unwrap();
        assert_eq!((unprinted.html.as_str(), unprinted.marks.len()), ("", 0));
    }

    #[test]
    fn shifts_a_mark_through_every_capture_it_travels_in() {
        let [default, name] = mark(0, 1);
        let page = rendered(
            vec![
                Value::html(""),
                Value::str("styles"),
                Value::str("xy"),
                Value::str("z"),
            ],
            vec![
                // The outer capture holds `xy`, then the inner capture's `z` and mark.
                Op::Capture,
                Op::EmitText { index: 2 },
                Op::Capture,
                Op::EmitText { index: 3 },
                default,
                name,
                Op::Emit,
                Op::EndCapture,
                Op::Emit,
                Op::EndCapture,
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(page.html, "xyz");
        assert_eq!(page.marks.iter().map(|m| m.start).collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn keeps_a_default_under_the_mark_that_stands_for_it() {
        let [default, name] = mark(0, 1);
        let page = rendered(
            vec![Value::html("<pre>code</pre>"), Value::str("code")],
            vec![default, name, Op::Emit],
        )
        .unwrap();
        assert_eq!(page.html, "<pre>code</pre>");
        assert_eq!(
            page.marks
                .iter()
                .map(|m| (m.start, m.end))
                .collect::<Vec<_>>(),
            [(0, 15)]
        );
    }

    #[test]
    fn keeps_two_marks_at_one_offset_in_the_order_they_were_printed() {
        let [default, styles] = mark(0, 1);
        let head = Op::Mark {
            name: 2,
            key: MarkKey::None,
            payload: MarkPayload::None,
        };
        let page = rendered(
            vec![Value::html(""), Value::str("styles"), Value::str("head")],
            vec![default, styles, Op::Emit, default, head, Op::Emit],
        )
        .unwrap();
        assert_eq!(page.html, "");
        let names: Vec<&str> = page.marks.iter().map(|m| &*m.name).collect();
        assert_eq!(names, ["styles", "head"]);
    }

    #[test]
    fn passes_a_marked_value_through_raw_unchanged() {
        let [default, name] = mark(0, 1);
        let page = rendered(
            vec![Value::html("<p>a</p>"), Value::str("toc")],
            vec![
                default,
                name,
                Op::CallBuiltin {
                    id: ambient("raw"),
                    argc: 1,
                },
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(page.html, "<p>a</p>");
        assert_eq!(page.marks.len(), 1);
    }

    #[test]
    fn refuses_a_mark_standing_in_another_marks_default() {
        let [default, name] = mark(0, 1);
        // At the second `Mark`, the third instruction.
        assert_eq!(
            error(rendered(
                vec![Value::html(""), Value::str("styles")],
                vec![
                    default,
                    name,
                    Op::Mark {
                        name: 1,
                        key: MarkKey::None,
                        payload: MarkPayload::None,
                    },
                ],
            )),
            (
                2,
                "a mark cannot stand inside another mark's default".to_owned()
            )
        );
    }

    #[test]
    fn refuses_a_marked_value_collected_into_a_fragment() {
        let [default, name] = mark(0, 1);
        assert_eq!(
            error(rendered(
                vec![Value::html(""), Value::str("styles")],
                vec![default, name, Op::Collect { name: 1 }],
            )),
            (2, "a mark cannot be collected into a fragment".to_owned())
        );
    }

    /// The cap is a stack budget, so the test that matters runs it on the stack it was
    /// budgeted against: the `.wasm`'s megabyte, pinned in `crates/lemmon-wasm/build.rs`,
    /// which is the smallest stack the core runs on and the one it cannot ask to grow.
    /// Overflowing aborts rather than failing, which is loud enough — and is what a
    /// browser would do with no diagnostic at all.
    #[test]
    fn nests_to_the_cap_within_the_stack_the_wasm_is_given() {
        const A_MEGABYTE: usize = 1024 * 1024;
        std::thread::Builder::new()
            .stack_size(A_MEGABYTE)
            .spawn(|| {
                let err = render(&nesting_module(), "t.lem", &[]).unwrap_err();
                assert_eq!(err.message, "builtins nested more than 64 deep");
            })
            .expect("a thread")
            .join()
            .expect("the deepest nesting the cap allows, on a megabyte of stack");
    }

    /// A module whose entry maps `f` over a one-element list, where `f` maps itself:
    /// every invoke pushes `f` again and the VM re-enters its loop once per level, so
    /// this is the native recursion [`MAX_NATIVE_DEPTH`] bounds.
    fn nesting_module() -> Module {
        let looping = function(
            "f",
            &["x"],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::Global { func: 1 },
                Op::CallBuiltin {
                    id: method_of(ReceiverKind::List, "map"),
                    argc: 2,
                },
                Op::Return,
            ],
        );
        let entry = function(
            "t.lem",
            &[],
            &[],
            vec![
                Op::Const { index: 0 },
                Op::Global { func: 1 },
                Op::CallBuiltin {
                    id: method_of(ReceiverKind::List, "map"),
                    argc: 2,
                },
                Op::Return,
            ],
        );
        let constants = vec![Value::list(vec![Value::Int(1)])];
        module(constants, vec![entry, looping])
    }

    fn record() -> Value {
        Value::map(
            [
                (Rc::from("title"), Value::str("T")),
                (Rc::from("sub"), Value::Null),
            ]
            .into_iter()
            .collect(),
        )
    }

    #[test]
    fn reads_a_field_by_name() {
        let out = run(
            vec![record(), Value::str("title")],
            vec![k(1), Op::GetField { name: 1 }, Op::Emit],
        )
        .unwrap();
        assert_eq!(out, "T");
    }

    #[test]
    fn reports_a_missing_field_and_a_non_map_object() {
        let (pc, message) = error(run(
            vec![record(), Value::str("nope")],
            vec![k(1), Op::GetField { name: 1 }],
        ));
        assert_eq!((pc, message.as_str()), (1, "no field `nope`"));
        let (_, message) = error(run(
            vec![Value::Int(1), Value::str("x")],
            vec![k(1), Op::GetField { name: 1 }],
        ));
        assert_eq!(message, "cannot read field `x` of a int");
        let (_, message) = error(run(
            vec![Value::str("x")],
            vec![k(0), Op::GetField { name: 0 }],
        ));
        assert_eq!(message, "cannot read field `x` of null");
    }

    #[test]
    fn propagates_null_through_an_optional_field_access_only() {
        let out = run(
            vec![Value::str("x"), Value::str("y")],
            vec![
                k(0),
                Op::OptField { name: 0 },
                Op::JumpIfFalse { to: 4 },
                Op::EmitText { index: 1 },
                Op::EmitText { index: 0 },
            ],
        )
        .unwrap();
        assert_eq!(out, "x");
        let (_, message) = error(run(
            vec![Value::Int(1), Value::str("x")],
            vec![k(1), Op::OptField { name: 1 }],
        ));
        assert_eq!(message, "cannot read field `x` of a int");
    }

    #[test]
    fn indexes_lists_and_maps() {
        let list = Value::list(vec![Value::Int(5), Value::Int(6)]);
        let out = run(
            vec![list, Value::Int(1), record(), Value::str("title")],
            vec![
                k(1),
                k(2),
                Op::Index,
                Op::Emit,
                k(3),
                k(4),
                Op::Index,
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "6T");
    }

    #[test]
    fn reports_every_way_an_index_can_fail() {
        let list = Value::list(vec![Value::Int(5)]);
        let (pc, message) = error(run(
            vec![list.clone(), Value::Int(1)],
            vec![k(1), k(2), Op::Index],
        ));
        assert_eq!(
            (pc, message.as_str()),
            (2, "index 1 is out of range for a list of 1")
        );
        let (_, message) = error(run(
            vec![list.clone(), Value::Int(-1)],
            vec![k(1), k(2), Op::Index],
        ));
        assert_eq!(message, "index -1 is out of range for a list of 1");
        let (_, message) = error(run(
            vec![record(), Value::str("z")],
            vec![k(1), k(2), Op::Index],
        ));
        assert_eq!(message, "no key \"z\"");
        let (_, message) = error(run(
            vec![list, Value::str("z")],
            vec![k(1), k(2), Op::Index],
        ));
        assert_eq!(message, "cannot index a list with a string");
    }

    #[test]
    fn reads_an_ambient_name_from_the_render_globals() {
        let out = run(
            vec![Value::str("page"), Value::str("site")],
            vec![Op::Ambient { name: 0 }, Op::Emit],
        )
        .unwrap();
        assert_eq!(out, "the page");
        let (pc, message) = error(run(
            vec![Value::str("page"), Value::str("site")],
            vec![Op::Ambient { name: 1 }],
        ));
        assert_eq!((pc, message.as_str()), (0, "`site` is not bound"));
    }

    #[test]
    fn iterates_a_list_with_and_without_the_index() {
        let list = Value::list(vec![Value::str("a"), Value::str("b")]);
        let out = run(
            vec![list, Value::str(",")],
            vec![
                k(1),
                Op::IterInit { exit: 7 },
                Op::IterNext {
                    exit: 7,
                    pair: true,
                },
                Op::Emit,
                Op::Emit,
                Op::EmitText { index: 1 },
                Op::Jump { to: 2 },
            ],
        )
        .unwrap();
        assert_eq!(out, "0a,1b,");
    }

    #[test]
    fn iterates_a_map_as_keys_or_as_pairs() {
        let map = Value::map(
            [
                (Rc::from("a"), Value::Int(1)),
                (Rc::from("b"), Value::Int(2)),
            ]
            .into_iter()
            .collect(),
        );
        let out = run(
            vec![map, Value::str("=")],
            vec![
                k(1),
                Op::IterInit { exit: 5 },
                Op::IterNext {
                    exit: 5,
                    pair: false,
                },
                Op::Emit,
                Op::Jump { to: 2 },
                k(1),
                Op::IterInit { exit: 12 },
                Op::IterNext {
                    exit: 12,
                    pair: true,
                },
                Op::Emit,
                Op::EmitText { index: 1 },
                Op::Emit,
                Op::Jump { to: 7 },
            ],
        )
        .unwrap();
        assert_eq!(out, "ab1=a2=b");
    }

    #[test]
    fn takes_the_empty_exit_without_opening_an_iterator() {
        let out = run(
            vec![Value::list(vec![]), Value::str("empty")],
            vec![
                k(1),
                Op::IterInit { exit: 5 },
                Op::IterNext {
                    exit: 6,
                    pair: false,
                },
                Op::Emit,
                Op::Jump { to: 2 },
                Op::EmitText { index: 1 },
            ],
        )
        .unwrap();
        assert_eq!(out, "empty");
    }

    #[test]
    fn refuses_to_iterate_a_non_collection() {
        let (pc, message) = error(run(
            vec![Value::Int(3)],
            vec![k(1), Op::IterInit { exit: 2 }],
        ));
        assert_eq!((pc, message.as_str()), (1, "cannot iterate over a int"));
    }

    #[test]
    fn coalesces_null_and_keeps_anything_else() {
        let out = run(
            vec![Value::str("a"), Value::str("b")],
            vec![
                k(0),
                Op::JumpUnlessNull { to: 3 },
                k(2),
                Op::Emit,
                k(1),
                Op::JumpUnlessNull { to: 7 },
                k(2),
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "ba");
    }

    #[test]
    fn short_circuits_and_and_or() {
        let out = run(
            vec![
                Value::Bool(false),
                Value::Bool(true),
                Value::str("t"),
                Value::str("f"),
            ],
            vec![
                k(1),
                Op::JumpIfFalseKeep { to: 3 },
                k(2),
                Op::JumpIfFalse { to: 6 },
                Op::EmitText { index: 2 },
                Op::Jump { to: 7 },
                Op::EmitText { index: 3 },
                k(2),
                Op::JumpIfTrueKeep { to: 10 },
                k(1),
                Op::JumpIfFalse { to: 13 },
                Op::EmitText { index: 2 },
                Op::Jump { to: 14 },
                Op::EmitText { index: 3 },
            ],
        )
        .unwrap();
        assert_eq!(out, "ft");
    }

    #[test]
    fn requires_bools_on_both_sides_of_a_logical_operator() {
        let (_, message) = error(run(
            vec![Value::Int(1)],
            vec![k(1), Op::JumpIfFalseKeep { to: 2 }],
        ));
        assert_eq!(message, "`&&` needs a bool, got int");
        let (_, message) = error(run(
            vec![Value::Int(1)],
            vec![k(1), Op::JumpIfTrueKeep { to: 2 }],
        ));
        assert_eq!(message, "`||` needs a bool, got int");
    }

    #[test]
    fn negates_numbers_and_bools_and_nothing_else() {
        let out = run(
            vec![
                Value::Int(3),
                Value::Float(1.5),
                Value::Bool(true),
                Value::str("y"),
            ],
            vec![
                k(1),
                Op::Neg,
                Op::Emit,
                k(2),
                Op::Neg,
                Op::Emit,
                k(3),
                Op::Not,
                Op::JumpIfFalse { to: 10 },
                Op::EmitText { index: 4 },
            ],
        )
        .unwrap();
        assert_eq!(out, "-3-1.5");
        let (_, message) = error(run(vec![Value::str("s")], vec![k(1), Op::Neg]));
        assert_eq!(message, "cannot negate a string");
        let (_, message) = error(run(vec![Value::Int(i64::MIN)], vec![k(1), Op::Neg]));
        assert_eq!(message, "integer overflow");
        let (_, message) = error(run(vec![Value::Int(1)], vec![k(1), Op::Not]));
        assert_eq!(message, "`!` needs a bool, got int");
    }

    fn arith(op: Op, lhs: Value, rhs: Value) -> Result<String, RuntimeError> {
        run(vec![lhs, rhs], vec![k(1), k(2), op, Op::Emit])
    }

    fn truth(op: Op, lhs: Value, rhs: Value) -> Result<String, RuntimeError> {
        run(
            vec![lhs, rhs, Value::str("t"), Value::str("f")],
            vec![
                k(1),
                k(2),
                op,
                Op::JumpIfFalse { to: 5 },
                Op::EmitText { index: 2 },
                Op::EmitText { index: 3 },
            ],
        )
        .map(|out| if out == "tf" { "t".to_owned() } else { out })
    }

    #[test]
    fn does_integer_arithmetic_with_overflow_checks() {
        assert_eq!(arith(Op::Add, Value::Int(2), Value::Int(3)).unwrap(), "5");
        assert_eq!(arith(Op::Sub, Value::Int(2), Value::Int(3)).unwrap(), "-1");
        assert_eq!(arith(Op::Mul, Value::Int(2), Value::Int(3)).unwrap(), "6");
        assert_eq!(arith(Op::Div, Value::Int(7), Value::Int(2)).unwrap(), "3");
        assert_eq!(arith(Op::Rem, Value::Int(7), Value::Int(2)).unwrap(), "1");
        let (pc, message) = error(arith(Op::Add, Value::Int(i64::MAX), Value::Int(1)));
        assert_eq!((pc, message.as_str()), (2, "integer overflow"));
        assert!(arith(Op::Sub, Value::Int(i64::MIN), Value::Int(1)).is_err());
        assert!(arith(Op::Mul, Value::Int(i64::MAX), Value::Int(2)).is_err());
        assert!(arith(Op::Div, Value::Int(i64::MIN), Value::Int(-1)).is_err());
        assert!(arith(Op::Rem, Value::Int(i64::MIN), Value::Int(-1)).is_err());
        let (_, message) = error(arith(Op::Div, Value::Int(1), Value::Int(0)));
        assert_eq!(message, "division by zero");
        assert!(arith(Op::Rem, Value::Int(1), Value::Int(0)).is_err());
    }

    #[test]
    fn promotes_to_float_when_either_side_is_one() {
        assert_eq!(
            arith(Op::Add, Value::Int(1), Value::Float(0.5)).unwrap(),
            "1.5"
        );
        assert_eq!(
            arith(Op::Sub, Value::Float(1.0), Value::Int(2)).unwrap(),
            "-1"
        );
        assert_eq!(
            arith(Op::Mul, Value::Float(1.5), Value::Float(2.0)).unwrap(),
            "3"
        );
        assert_eq!(
            arith(Op::Div, Value::Float(1.0), Value::Float(4.0)).unwrap(),
            "0.25"
        );
        assert_eq!(
            arith(Op::Rem, Value::Float(5.5), Value::Float(2.0)).unwrap(),
            "1.5"
        );
        let (_, message) = error(arith(Op::Div, Value::Float(1.0), Value::Float(0.0)));
        assert_eq!(message, "division by zero");
        assert_eq!(
            truth(Op::Lt, Value::Int(1), Value::Float(1.5)).unwrap(),
            "t"
        );
    }

    #[test]
    fn refuses_to_order_a_nan() {
        let out = run(
            vec![Value::Float(f64::MAX), Value::Float(10.0), Value::Int(1)],
            vec![
                k(1),
                k(2),
                Op::Mul,
                k(1),
                k(2),
                Op::Mul,
                Op::Sub,
                k(3),
                Op::Lt,
            ],
        );
        let (pc, message) = error(out);
        assert_eq!((pc, message.as_str()), (8, "cannot order a NaN"));
    }

    #[test]
    fn concatenates_strings_and_orders_them() {
        assert_eq!(
            arith(Op::Add, Value::str("a<"), Value::str("b")).unwrap(),
            "a&lt;b"
        );
        assert_eq!(
            truth(Op::Lt, Value::str("a"), Value::str("b")).unwrap(),
            "t"
        );
        assert_eq!(
            truth(Op::Ge, Value::str("a"), Value::str("b")).unwrap(),
            "f"
        );
    }

    #[test]
    fn compares_numbers_with_every_ordering_operator() {
        assert_eq!(truth(Op::Lt, Value::Int(1), Value::Int(2)).unwrap(), "t");
        assert_eq!(truth(Op::Le, Value::Int(2), Value::Int(2)).unwrap(), "t");
        assert_eq!(truth(Op::Gt, Value::Int(1), Value::Int(2)).unwrap(), "f");
        assert_eq!(truth(Op::Ge, Value::Int(1), Value::Int(2)).unwrap(), "f");
        assert_eq!(truth(Op::Eq, Value::Int(1), Value::Int(1)).unwrap(), "t");
        assert_eq!(truth(Op::Ne, Value::Int(1), Value::Int(1)).unwrap(), "f");
    }

    #[test]
    fn tests_membership_in_lists_and_maps() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(truth(Op::In, Value::Int(2), list.clone()).unwrap(), "t");
        assert_eq!(truth(Op::In, Value::Int(3), list).unwrap(), "f");
        assert_eq!(truth(Op::In, Value::str("title"), record()).unwrap(), "t");
        let (_, message) = error(truth(Op::In, Value::Int(1), Value::Int(2)));
        assert_eq!(message, "cannot apply `in` to a int and a int");
    }

    #[test]
    fn names_the_operator_and_both_types_in_a_mismatch() {
        let (_, message) = error(arith(Op::Add, Value::str("a"), Value::Int(1)));
        assert_eq!(message, "cannot apply `+` to a string and a int");
        for (op, name) in [
            (Op::Sub, "-"),
            (Op::Mul, "*"),
            (Op::Div, "/"),
            (Op::Rem, "%"),
            (Op::Lt, "lt"),
            (Op::Le, "lte"),
            (Op::Gt, "gt"),
            (Op::Ge, "gte"),
        ] {
            let (_, message) = error(arith(op, Value::Bool(true), Value::Int(1)));
            assert_eq!(
                message,
                format!("cannot apply `{name}` to a bool and a int")
            );
        }
        let (_, message) = error(truth(
            Op::Eq,
            Value::Null,
            Value::Fn(Rc::new(Closure {
                func: 0,
                captures: vec![],
            })),
        ));
        assert_eq!(message, "functions cannot be compared");
    }

    #[test]
    fn builds_lists_and_maps_from_the_stack() {
        let out = run(
            vec![Value::str("k"), Value::Int(1), Value::Int(2), Value::Int(0)],
            vec![
                k(2),
                k(3),
                Op::MakeList { len: 2 },
                k(4),
                Op::Index,
                Op::Emit,
                k(1),
                k(3),
                Op::MakeMap { len: 1 },
                k(1),
                Op::Index,
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "12");
    }

    /// `Const "title"; Const "T"; MakeRecord LinkPost` — a link post on the stack.
    fn link_post() -> Vec<Op> {
        vec![k(1), k(2), Op::MakeRecord { name: 2, len: 1 }]
    }

    /// The constants `link_post` reads: `constants[1]` is `"title"`, `[2]` `"T"`, and
    /// the 0-based name index 2 shifts to `[3]`, `"LinkPost"`.
    fn record_constants() -> Vec<Value> {
        vec![Value::str("title"), Value::str("T"), Value::str("LinkPost")]
    }

    #[test]
    fn builds_a_record_from_the_stack_and_reads_its_fields_by_name() {
        let mut code = link_post();
        code.extend([Op::GetField { name: 0 }, Op::Emit]);
        assert_eq!(run(record_constants(), code).unwrap(), "T");
        let mut code = link_post();
        code.extend([Op::GetField { name: 1 }, Op::Emit]);
        let (_, message) = error(run(record_constants(), code));
        assert_eq!(message, "no field `T` on `LinkPost` record");
    }

    /// The constants a type-test case reads past `record_constants`: names to test
    /// against at 0-based 3 and 4, and the words the bool is printed as.
    fn type_test_constants() -> Vec<Value> {
        let mut constants = record_constants();
        constants.extend([
            Value::str("Article"),
            Value::str("Photo"),
            Value::str("true"),
            Value::str("false"),
        ]);
        constants
    }

    /// Prints the bool on top of the stack as `true` or `false`, from pc 4.
    fn print_bool() -> [Op; 5] {
        [
            Op::JumpIfFalse { to: 7 },
            k(6),
            Op::Jump { to: 8 },
            k(7),
            Op::Emit,
        ]
    }

    #[test]
    fn tests_a_records_type_through_the_ancestry_table() {
        for (name, expected) in [(2, "true"), (3, "true"), (4, "false")] {
            let mut code = link_post();
            code.push(Op::TypeTest { name });
            code.extend(print_bool());
            assert_eq!(
                run(type_test_constants(), code).unwrap(),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn finds_a_null_to_be_no_record_and_refuses_to_ask_a_map() {
        let mut code = vec![k(0), Op::TypeTest { name: 2 }, k(0), k(0)];
        code.extend(print_bool());
        assert_eq!(run(type_test_constants(), code).unwrap(), "false");
        let mut constants = type_test_constants();
        constants.push(Value::map(Map::default()));
        let (pc, message) = error(run(
            constants,
            vec![k(8), Op::TypeTest { name: 2 }, Op::Emit],
        ));
        assert_eq!(pc, 1);
        assert_eq!(
            message,
            "cannot test whether a map is `LinkPost`: only a record carries its type"
        );
    }

    #[test]
    fn calls_a_records_field_through_any_where_a_map_takes_the_method() {
        // The `any` receiver path agrees with the typed one: a record's field holds
        // the callee — here an int, so the call fails as a call — while a map takes the
        // method of its runtime type even when a key spells the method's name.
        let constants = vec![
            Value::str("keys"),
            Value::Int(1),
            Value::str("LinkPost"),
            Value::Int(0),
        ];
        let (pc, message) = error(run(
            constants.clone(),
            vec![
                k(1),
                k(2),
                Op::MakeRecord { name: 2, len: 1 },
                Op::CallDynamic { name: 0, argc: 0 },
            ],
        ));
        assert_eq!((pc, message.as_str()), (3, "cannot call a int"));
        let out = run(
            constants,
            vec![
                k(1),
                k(2),
                Op::MakeMap { len: 1 },
                Op::CallDynamic { name: 0, argc: 0 },
                k(4),
                Op::Index,
                Op::Emit,
            ],
        )
        .unwrap();
        assert_eq!(out, "keys");
    }
}
