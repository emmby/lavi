#![allow(clippy::useless_conversion)]
#![allow(clippy::arc_with_non_send_sync)]

extern crate alloc;

use indexmap::IndexMap;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Iterative Value Conversions (Zero CPU Call-Stack Recursion)
// ---------------------------------------------------------------------------

fn core_value_to_python(val: &lavi::Value, py: Python<'_>) -> PyResult<PyObject> {
    match val {
        lavi::Value::Null => return Ok(py.None()),
        lavi::Value::Bool(b) => return Ok(b.into_py(py)),
        lavi::Value::Number(n) => {
            if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                return Ok((*n as i64).into_py(py));
            } else {
                return Ok((*n).into_py(py));
            }
        }
        lavi::Value::String(s) => return Ok(s.into_py(py)),
        lavi::Value::Array(a) if a.is_empty() => {
            return Ok(PyList::empty_bound(py).into_py(py));
        }
        lavi::Value::Object(o) if o.is_empty() => {
            return Ok(PyDict::new_bound(py).into_py(py));
        }
        _ => {}
    }

    enum PyConvertFrame<'a> {
        Array {
            iter: core::slice::Iter<'a, lavi::Value>,
            py_list: Bound<'a, PyList>,
        },
        Object {
            iter: indexmap::map::Iter<'a, String, lavi::Value>,
            py_dict: Bound<'a, PyDict>,
            pending_key: Option<&'a str>,
        },
    }

    let mut stack: Vec<PyConvertFrame<'_>> = Vec::with_capacity(16);
    let mut completed: Option<Bound<'_, PyAny>> = None;

    match val {
        lavi::Value::Array(a) => {
            stack.push(PyConvertFrame::Array {
                iter: a.iter(),
                py_list: PyList::empty_bound(py),
            });
        }
        lavi::Value::Object(o) => {
            stack.push(PyConvertFrame::Object {
                iter: o.iter(),
                py_dict: PyDict::new_bound(py),
                pending_key: None,
            });
        }
        _ => unreachable!(),
    }

    loop {
        if let Some(comp) = completed.take() {
            if let Some(top) = stack.last_mut() {
                match top {
                    PyConvertFrame::Array { py_list, .. } => {
                        py_list.append(comp)?;
                    }
                    PyConvertFrame::Object {
                        py_dict,
                        pending_key,
                        ..
                    } => {
                        let key = pending_key.take().expect("pending_key must exist");
                        py_dict.set_item(key, comp)?;
                    }
                }
            } else {
                return Ok(comp.into_py(py));
            }
        }

        let top = match stack.last_mut() {
            Some(t) => t,
            None => break,
        };

        match top {
            PyConvertFrame::Array { iter, .. } => {
                if let Some(next_item) = iter.next() {
                    match next_item {
                        lavi::Value::Null => completed = Some(py.None().into_bound(py)),
                        lavi::Value::Bool(b) => completed = Some(b.into_py(py).into_bound(py)),
                        lavi::Value::Number(n) => {
                            let obj = if n.fract() == 0.0
                                && *n >= (i64::MIN as f64)
                                && *n <= (i64::MAX as f64)
                            {
                                (*n as i64).into_py(py)
                            } else {
                                (*n).into_py(py)
                            };
                            completed = Some(obj.into_bound(py));
                        }
                        lavi::Value::String(s) => completed = Some(s.into_py(py).into_bound(py)),
                        lavi::Value::Array(a) if a.is_empty() => {
                            completed = Some(PyList::empty_bound(py).into_any());
                        }
                        lavi::Value::Array(a) => {
                            let sub_list = PyList::empty_bound(py);
                            stack.push(PyConvertFrame::Array {
                                iter: a.iter(),
                                py_list: sub_list,
                            });
                        }
                        lavi::Value::Object(o) if o.is_empty() => {
                            completed = Some(PyDict::new_bound(py).into_any());
                        }
                        lavi::Value::Object(o) => {
                            let sub_dict = PyDict::new_bound(py);
                            stack.push(PyConvertFrame::Object {
                                iter: o.iter(),
                                py_dict: sub_dict,
                                pending_key: None,
                            });
                        }
                    }
                } else {
                    let frame = stack.pop().unwrap();
                    if let PyConvertFrame::Array { py_list, .. } = frame {
                        completed = Some(py_list.into_any());
                    }
                }
            }
            PyConvertFrame::Object {
                iter, pending_key, ..
            } => {
                if let Some((key, next_item)) = iter.next() {
                    *pending_key = Some(key.as_str());
                    match next_item {
                        lavi::Value::Null => completed = Some(py.None().into_bound(py)),
                        lavi::Value::Bool(b) => completed = Some(b.into_py(py).into_bound(py)),
                        lavi::Value::Number(n) => {
                            let obj = if n.fract() == 0.0
                                && *n >= (i64::MIN as f64)
                                && *n <= (i64::MAX as f64)
                            {
                                (*n as i64).into_py(py)
                            } else {
                                (*n).into_py(py)
                            };
                            completed = Some(obj.into_bound(py));
                        }
                        lavi::Value::String(s) => completed = Some(s.into_py(py).into_bound(py)),
                        lavi::Value::Array(a) if a.is_empty() => {
                            completed = Some(PyList::empty_bound(py).into_any());
                        }
                        lavi::Value::Array(a) => {
                            let sub_list = PyList::empty_bound(py);
                            stack.push(PyConvertFrame::Array {
                                iter: a.iter(),
                                py_list: sub_list,
                            });
                        }
                        lavi::Value::Object(o) if o.is_empty() => {
                            completed = Some(PyDict::new_bound(py).into_any());
                        }
                        lavi::Value::Object(o) => {
                            let sub_dict = PyDict::new_bound(py);
                            stack.push(PyConvertFrame::Object {
                                iter: o.iter(),
                                py_dict: sub_dict,
                                pending_key: None,
                            });
                        }
                    }
                } else {
                    let frame = stack.pop().unwrap();
                    if let PyConvertFrame::Object { py_dict, .. } = frame {
                        completed = Some(py_dict.into_any());
                    }
                }
            }
        }
    }

    if let Some(comp) = completed {
        Ok(comp.into_py(py))
    } else {
        Ok(py.None())
    }
}

fn python_to_core_value(obj: &Bound<'_, PyAny>) -> PyResult<lavi::Value> {
    if obj.is_none() {
        return Ok(lavi::Value::Null);
    }
    if let Ok(val) = obj.extract::<PyRef<Value>>() {
        return Ok((*val.0).clone());
    }
    if obj.is_instance_of::<pyo3::types::PyBool>() {
        return Ok(lavi::Value::Bool(obj.extract::<bool>()?));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(lavi::Value::Number(i as f64));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(lavi::Value::Number(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(lavi::Value::String(s));
    }

    enum PyToValFrame<'py> {
        List {
            items: Vec<Bound<'py, PyAny>>,
            idx: usize,
            out: Vec<lavi::Value>,
        },
        Dict {
            items: Vec<(String, Bound<'py, PyAny>)>,
            idx: usize,
            out: IndexMap<String, lavi::Value>,
            pending_key: Option<String>,
        },
    }

    let mut stack: Vec<PyToValFrame<'_>> = Vec::with_capacity(16);
    let mut completed: Option<lavi::Value> = None;

    if let Ok(dict) = obj.downcast::<PyDict>() {
        if dict.is_empty() {
            return Ok(lavi::Value::Object(IndexMap::new()));
        }
        let mut items = Vec::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            items.push((k.extract::<String>()?, v));
        }
        stack.push(PyToValFrame::Dict {
            items,
            idx: 0,
            out: IndexMap::with_capacity(dict.len()),
            pending_key: None,
        });
    } else if let Ok(list) = obj.downcast::<PyList>() {
        if list.is_empty() {
            return Ok(lavi::Value::Array(Vec::new()));
        }
        let items: Vec<Bound<'_, PyAny>> = list.iter().collect();
        stack.push(PyToValFrame::List {
            items,
            idx: 0,
            out: Vec::with_capacity(list.len()),
        });
    } else if let Ok(tuple) = obj.downcast::<PyTuple>() {
        if tuple.is_empty() {
            return Ok(lavi::Value::Array(Vec::new()));
        }
        let items: Vec<Bound<'_, PyAny>> = tuple.iter().collect();
        stack.push(PyToValFrame::List {
            items,
            idx: 0,
            out: Vec::with_capacity(tuple.len()),
        });
    } else {
        return Err(PyTypeError::new_err(format!(
            "Unsupported Python type for Value: {}",
            obj.get_type().name()?
        )));
    }

    loop {
        if let Some(comp) = completed.take() {
            if let Some(top) = stack.last_mut() {
                match top {
                    PyToValFrame::List { out, .. } => {
                        out.push(comp);
                    }
                    PyToValFrame::Dict {
                        out, pending_key, ..
                    } => {
                        let key = pending_key.take().expect("pending_key must exist");
                        out.insert(key, comp);
                    }
                }
            } else {
                return Ok(comp);
            }
        }

        let top = match stack.last_mut() {
            Some(t) => t,
            None => break,
        };

        match top {
            PyToValFrame::List { items, idx, .. } => {
                if *idx < items.len() {
                    let next_item = items[*idx].clone();
                    *idx += 1;

                    if next_item.is_none() {
                        completed = Some(lavi::Value::Null);
                    } else if let Ok(val) = next_item.extract::<PyRef<Value>>() {
                        completed = Some((*val.0).clone());
                    } else if next_item.is_instance_of::<pyo3::types::PyBool>() {
                        completed = Some(lavi::Value::Bool(next_item.extract::<bool>()?));
                    } else if let Ok(i) = next_item.extract::<i64>() {
                        completed = Some(lavi::Value::Number(i as f64));
                    } else if let Ok(f) = next_item.extract::<f64>() {
                        completed = Some(lavi::Value::Number(f));
                    } else if let Ok(s) = next_item.extract::<String>() {
                        completed = Some(lavi::Value::String(s));
                    } else if let Ok(sub_dict) = next_item.downcast::<PyDict>() {
                        if sub_dict.is_empty() {
                            completed = Some(lavi::Value::Object(IndexMap::new()));
                        } else {
                            let mut sub_items = Vec::with_capacity(sub_dict.len());
                            for (k, v) in sub_dict.iter() {
                                sub_items.push((k.extract::<String>()?, v));
                            }
                            stack.push(PyToValFrame::Dict {
                                items: sub_items,
                                idx: 0,
                                out: IndexMap::with_capacity(sub_dict.len()),
                                pending_key: None,
                            });
                        }
                    } else if let Ok(sub_list) = next_item.downcast::<PyList>() {
                        if sub_list.is_empty() {
                            completed = Some(lavi::Value::Array(Vec::new()));
                        } else {
                            let sub_items: Vec<Bound<'_, PyAny>> = sub_list.iter().collect();
                            stack.push(PyToValFrame::List {
                                items: sub_items,
                                idx: 0,
                                out: Vec::with_capacity(sub_list.len()),
                            });
                        }
                    } else if let Ok(sub_tuple) = next_item.downcast::<PyTuple>() {
                        if sub_tuple.is_empty() {
                            completed = Some(lavi::Value::Array(Vec::new()));
                        } else {
                            let sub_items: Vec<Bound<'_, PyAny>> = sub_tuple.iter().collect();
                            stack.push(PyToValFrame::List {
                                items: sub_items,
                                idx: 0,
                                out: Vec::with_capacity(sub_tuple.len()),
                            });
                        }
                    } else {
                        return Err(PyTypeError::new_err(format!(
                            "Unsupported Python type for Value in array: {}",
                            next_item.get_type().name()?
                        )));
                    }
                } else {
                    let frame = stack.pop().unwrap();
                    if let PyToValFrame::List { out, .. } = frame {
                        completed = Some(lavi::Value::Array(out));
                    }
                }
            }
            PyToValFrame::Dict {
                items,
                idx,
                pending_key,
                ..
            } => {
                if *idx < items.len() {
                    let (k_str, v_obj) = items[*idx].clone();
                    *idx += 1;
                    *pending_key = Some(k_str);

                    if v_obj.is_none() {
                        completed = Some(lavi::Value::Null);
                    } else if let Ok(val) = v_obj.extract::<PyRef<Value>>() {
                        completed = Some((*val.0).clone());
                    } else if v_obj.is_instance_of::<pyo3::types::PyBool>() {
                        completed = Some(lavi::Value::Bool(v_obj.extract::<bool>()?));
                    } else if let Ok(i) = v_obj.extract::<i64>() {
                        completed = Some(lavi::Value::Number(i as f64));
                    } else if let Ok(f) = v_obj.extract::<f64>() {
                        completed = Some(lavi::Value::Number(f));
                    } else if let Ok(s) = v_obj.extract::<String>() {
                        completed = Some(lavi::Value::String(s));
                    } else if let Ok(sub_dict) = v_obj.downcast::<PyDict>() {
                        if sub_dict.is_empty() {
                            completed = Some(lavi::Value::Object(IndexMap::new()));
                        } else {
                            let mut sub_items = Vec::with_capacity(sub_dict.len());
                            for (k, v) in sub_dict.iter() {
                                sub_items.push((k.extract::<String>()?, v));
                            }
                            stack.push(PyToValFrame::Dict {
                                items: sub_items,
                                idx: 0,
                                out: IndexMap::with_capacity(sub_dict.len()),
                                pending_key: None,
                            });
                        }
                    } else if let Ok(sub_list) = v_obj.downcast::<PyList>() {
                        if sub_list.is_empty() {
                            completed = Some(lavi::Value::Array(Vec::new()));
                        } else {
                            let sub_items: Vec<Bound<'_, PyAny>> = sub_list.iter().collect();
                            stack.push(PyToValFrame::List {
                                items: sub_items,
                                idx: 0,
                                out: Vec::with_capacity(sub_list.len()),
                            });
                        }
                    } else if let Ok(sub_tuple) = v_obj.downcast::<PyTuple>() {
                        if sub_tuple.is_empty() {
                            completed = Some(lavi::Value::Array(Vec::new()));
                        } else {
                            let sub_items: Vec<Bound<'_, PyAny>> = sub_tuple.iter().collect();
                            stack.push(PyToValFrame::List {
                                items: sub_items,
                                idx: 0,
                                out: Vec::with_capacity(sub_tuple.len()),
                            });
                        }
                    } else {
                        return Err(PyTypeError::new_err(format!(
                            "Unsupported Python type for Value in dict: {}",
                            v_obj.get_type().name()?
                        )));
                    }
                } else {
                    let frame = stack.pop().unwrap();
                    if let PyToValFrame::Dict { out, .. } = frame {
                        completed = Some(lavi::Value::Object(out));
                    }
                }
            }
        }
    }

    if let Some(comp) = completed {
        Ok(comp)
    } else {
        Ok(lavi::Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Value pyclass
// ---------------------------------------------------------------------------

#[pyclass]
#[derive(Clone)]
pub struct Value(pub Arc<lavi::Value>);

#[pymethods]
impl Value {
    #[new]
    #[pyo3(signature = (val=None))]
    fn new(val: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        match val {
            None => Ok(Value(Arc::new(lavi::Value::Null))),
            Some(v) => {
                let core_val = python_to_core_value(v)?;
                Ok(Value(Arc::new(core_val)))
            }
        }
    }

    fn is_null(&self) -> bool {
        matches!(*self.0, lavi::Value::Null)
    }

    fn is_bool(&self) -> bool {
        matches!(*self.0, lavi::Value::Bool(_))
    }

    fn is_number(&self) -> bool {
        matches!(*self.0, lavi::Value::Number(_))
    }

    fn is_str(&self) -> bool {
        matches!(*self.0, lavi::Value::String(_))
    }

    fn is_array(&self) -> bool {
        matches!(*self.0, lavi::Value::Array(_))
    }

    fn is_object(&self) -> bool {
        matches!(*self.0, lavi::Value::Object(_))
    }

    fn as_bool(&self) -> Option<bool> {
        self.0.as_bool()
    }

    fn as_float(&self) -> Option<f64> {
        self.0.as_f64()
    }

    fn as_int(&self) -> Option<i64> {
        self.0.as_f64().map(|n| n as i64)
    }

    fn as_str(&self) -> Option<String> {
        self.0.as_string().cloned()
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, key: &str, default: Option<PyObject>, py: Python<'_>) -> PyObject {
        if let lavi::Value::Object(map) = &*self.0 {
            if let Some(v) = map.get(key) {
                return Value(Arc::new(v.clone())).into_py(py);
            }
        }
        default.unwrap_or_else(|| py.None())
    }

    fn __getitem__(&self, key: &Bound<'_, PyAny>) -> Value {
        if let Ok(idx) = key.extract::<isize>() {
            if let lavi::Value::Array(arr) = &*self.0 {
                let resolved_idx = if idx < 0 {
                    (arr.len() as isize)
                        .checked_add(idx)
                        .filter(|&p| p >= 0)
                        .map(|p| p as usize)
                } else {
                    Some(idx as usize)
                };
                if let Some(actual_idx) = resolved_idx {
                    if actual_idx < arr.len() {
                        return Value(Arc::new(arr[actual_idx].clone()));
                    }
                }
            }
            return Value(Arc::new(lavi::Value::Null));
        }

        if let Ok(key_str) = key.extract::<String>() {
            if let lavi::Value::Object(map) = &*self.0 {
                if let Some(v) = map.get(&key_str) {
                    return Value(Arc::new(v.clone()));
                }
            }
            return Value(Arc::new(lavi::Value::Null));
        }

        Value(Arc::new(lavi::Value::Null))
    }

    fn __contains__(&self, key: &str) -> bool {
        self.0.as_object().is_some_and(|m| m.contains_key(key))
    }

    fn __len__(&self) -> usize {
        match &*self.0 {
            lavi::Value::Array(arr) => arr.len(),
            lavi::Value::Object(map) => map.len(),
            _ => 0,
        }
    }

    fn __bool__(&self) -> bool {
        match &*self.0 {
            lavi::Value::Null => false,
            lavi::Value::Bool(b) => *b,
            lavi::Value::Number(n) => *n != 0.0 && !n.is_nan(),
            lavi::Value::String(s) => !s.is_empty(),
            lavi::Value::Array(arr) => !arr.is_empty(),
            lavi::Value::Object(map) => !map.is_empty(),
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other_val) = other.extract::<PyRef<Value>>() {
            return *self.0 == *other_val.0;
        }

        match &*self.0 {
            lavi::Value::Null => other.is_none(),
            lavi::Value::Bool(b) => {
                if other.is_instance_of::<pyo3::types::PyBool>() {
                    if let Ok(b2) = other.extract::<bool>() {
                        return *b == b2;
                    }
                }
                false
            }
            lavi::Value::Number(n) => {
                if other.is_instance_of::<pyo3::types::PyBool>() {
                    return false;
                }
                if let Ok(i) = other.extract::<i64>() {
                    (*n == i as f64) && (n.fract() == 0.0)
                } else if let Ok(f) = other.extract::<f64>() {
                    *n == f
                } else {
                    false
                }
            }
            lavi::Value::String(s) => {
                if let Ok(s2) = other.extract::<String>() {
                    *s == s2
                } else {
                    false
                }
            }
            lavi::Value::Array(_) | lavi::Value::Object(_) => {
                if let Ok(py_obj) = self.to_python(other.py()) {
                    other.eq(py_obj).unwrap_or_default()
                } else {
                    false
                }
            }
        }
    }

    fn __repr__(&self) -> String {
        format!("Value({})", self.0.to_json_string())
    }

    fn __str__(&self) -> String {
        self.0.to_json_string()
    }

    fn to_python(&self, py: Python<'_>) -> PyResult<PyObject> {
        core_value_to_python(&self.0, py)
    }
}

// ---------------------------------------------------------------------------
// SimulatedClock pyclass
// ---------------------------------------------------------------------------

pub struct SimulatedClockInner {
    pub clock: lavi::SimulatedClock,
    pub closed: bool,
    pub machines: Vec<Weak<MachineState>>,
}

#[pyclass(unsendable)]
#[derive(Clone)]
pub struct SimulatedClock {
    pub inner: Arc<Mutex<SimulatedClockInner>>,
}

#[pymethods]
impl SimulatedClock {
    #[new]
    #[pyo3(signature = (initial_ms=0))]
    fn new(initial_ms: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SimulatedClockInner {
                clock: lavi::SimulatedClock::with_initial(initial_ms),
                closed: false,
                machines: Vec::new(),
            })),
        }
    }

    fn now(&self) -> PyResult<u64> {
        let guard = self.inner.lock().unwrap();
        if guard.closed {
            return Err(PyRuntimeError::new_err("SimulatedClock is closed"));
        }
        Ok(guard.clock.now())
    }

    fn set(&self, target_ms: u64) -> PyResult<()> {
        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;

        loop {
            let (clock, machines_to_step) = {
                let mut guard = self.inner.lock().unwrap();
                if guard.closed {
                    return Err(PyRuntimeError::new_err("SimulatedClock is closed"));
                }
                guard.machines.retain(|weak| weak.upgrade().is_some());
                let current: Vec<Rc<MachineState>> =
                    guard.machines.iter().filter_map(|w| w.upgrade()).collect();
                (guard.clock.clone(), current)
            };

            let mut min_deadline: Option<u64> = None;
            for m_state in &machines_to_step {
                if let Ok(Some(d)) = m_state.get_next_deadline() {
                    min_deadline = Some(min_deadline.map_or(d, |curr| curr.min(d)));
                }
            }

            let next_deadline = match min_deadline {
                Some(d) => d,
                None => break,
            };

            if next_deadline > target_ms {
                break;
            }

            if last_deadline == Some(next_deadline) {
                same_deadline_steps += 1;
                if same_deadline_steps >= 100 {
                    break;
                }
            } else {
                last_deadline = Some(next_deadline);
                same_deadline_steps = 0;
            }

            total_steps += 1;
            if total_steps > 1_000_000 {
                break;
            }

            clock.set_now_raw(next_deadline);

            for m_state in &machines_to_step {
                m_state.step_tick()?;
            }
        }

        let guard = self.inner.lock().unwrap();
        if guard.closed {
            return Err(PyRuntimeError::new_err("SimulatedClock is closed"));
        }
        guard.clock.set_now_raw(target_ms);

        Ok(())
    }

    fn increment(&self, dt_ms: u64) -> PyResult<()> {
        let now = self.now()?;
        self.set(now.saturating_add(dt_ms))
    }

    fn close(&self) -> PyResult<()> {
        let mut guard = self.inner.lock().unwrap();
        guard.closed = true;
        guard.machines.clear();
        Ok(())
    }

    fn __enter__<'py>(slf: &Bound<'py, Self>) -> Bound<'py, Self> {
        slf.clone()
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close()
    }

    fn __traverse__(&self, _visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        Ok(())
    }

    fn __clear__(&mut self) {
        let mut guard = self.inner.lock().unwrap();
        guard.machines.clear();
    }
}

// ---------------------------------------------------------------------------
// Machine pyclass and MachineState
// ---------------------------------------------------------------------------

pub enum PendingCommand {
    Send {
        event_name: String,
        payload_val: lavi::Value,
    },
    Stop,
}

pub struct MachineState {
    pub machine: RefCell<Option<lavi::RuntimeMachine>>,
    pub active_machine: Cell<*const lavi::RuntimeMachine>,
    pub callback_depth: Cell<usize>,
    pub aborted: Cell<bool>,
    pub pending_commands: RefCell<VecDeque<PendingCommand>>,
    pub python_actions: RefCell<IndexMap<String, Py<PyAny>>>,
    pub python_guards: RefCell<IndexMap<String, Py<PyAny>>>,
    pub python_clock: RefCell<Option<Py<PyAny>>>,
    pub guards: RefCell<Rc<IndexMap<String, lavi::GuardFunction>>>,
    pub actions: RefCell<Rc<IndexMap<String, lavi::ActionFunction>>>,
    pub clock: RefCell<Option<SimulatedClock>>,
    pub last_error: RefCell<Option<PyErr>>,
    pub is_closed: Cell<bool>,
}

struct CallbackGuard<'a> {
    depth: &'a Cell<usize>,
}

impl<'a> CallbackGuard<'a> {
    fn new(depth: &'a Cell<usize>) -> Self {
        depth.set(depth.get().saturating_add(1));
        Self { depth }
    }
}

impl<'a> Drop for CallbackGuard<'a> {
    fn drop(&mut self) {
        let current = self.depth.get();
        if current > 0 {
            self.depth.set(current - 1);
        }
    }
}

impl MachineState {
    fn is_in_callback(&self) -> bool {
        self.callback_depth.get() > 0
    }

    fn with_machine<R>(&self, f: impl FnOnce(&lavi::RuntimeMachine) -> R) -> PyResult<R> {
        if self.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }
        if self.is_in_callback() {
            let ptr = self.active_machine.get();
            if !ptr.is_null() {
                let m = unsafe { &*ptr };
                return Ok(f(m));
            }
        }
        let guard = self.machine.borrow();
        match guard.as_ref() {
            Some(m) => Ok(f(m)),
            None => Err(PyRuntimeError::new_err("Machine is closed")),
        }
    }

    fn check_and_raise_error(&self) -> PyResult<()> {
        if self.aborted.get() {
            self.aborted.set(false);
            self.pending_commands.borrow_mut().clear();
            if let Some(err) = self.last_error.borrow_mut().take() {
                return Err(err);
            }
        }
        Ok(())
    }

    fn step_transition(&self, event_name: &str, payload_val: &lavi::Value) -> PyResult<()> {
        if self.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }
        if self.aborted.get() {
            return self.check_and_raise_error();
        }

        let guards = Rc::clone(&*self.guards.borrow());
        let actions = Rc::clone(&*self.actions.borrow());

        {
            let mut guard = self.machine.borrow_mut();
            let machine_ref = guard
                .as_mut()
                .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
            if !machine_ref.is_started || machine_ref.status == lavi::MachineStatus::Stopped {
                return Ok(());
            }
            self.active_machine
                .set(machine_ref as *const lavi::RuntimeMachine);
            machine_ref.transition(event_name, payload_val, &guards, &actions);
            self.active_machine.set(std::ptr::null());
        }

        self.check_and_raise_error()?;
        self.drain_pending_commands()
    }

    fn step_tick(&self) -> PyResult<()> {
        if self.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }
        if self.aborted.get() {
            return self.check_and_raise_error();
        }

        let guards = Rc::clone(&*self.guards.borrow());
        let actions = Rc::clone(&*self.actions.borrow());

        {
            let mut guard = self.machine.borrow_mut();
            let machine_ref = guard
                .as_mut()
                .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
            if !machine_ref.is_started
                || machine_ref.status == lavi::MachineStatus::Stopped
                || machine_ref.is_done()
                || machine_ref.status == lavi::MachineStatus::Done
            {
                return Ok(());
            }
            self.active_machine
                .set(machine_ref as *const lavi::RuntimeMachine);
            let _ = machine_ref.tick(&guards, &actions);
            self.active_machine.set(std::ptr::null());
        }

        self.check_and_raise_error()?;
        self.drain_pending_commands()
    }

    fn step_start(&self) -> PyResult<()> {
        if self.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }
        if self.aborted.get() {
            return self.check_and_raise_error();
        }

        let guards = Rc::clone(&*self.guards.borrow());
        let actions = Rc::clone(&*self.actions.borrow());

        {
            let mut guard = self.machine.borrow_mut();
            let machine_ref = guard
                .as_mut()
                .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
            if !machine_ref.is_started {
                self.active_machine
                    .set(machine_ref as *const lavi::RuntimeMachine);
                machine_ref.start(&guards, &actions);
                self.active_machine.set(std::ptr::null());
            }
        }

        self.check_and_raise_error()?;
        self.drain_pending_commands()
    }

    fn step_stop(&self) -> PyResult<()> {
        if self.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }
        let mut guard = self.machine.borrow_mut();
        if let Some(ref mut m) = *guard {
            m.stop();
        }
        Ok(())
    }

    fn drain_pending_commands(&self) -> PyResult<()> {
        loop {
            if self.aborted.get() {
                self.pending_commands.borrow_mut().clear();
                break;
            }
            let next_cmd = self.pending_commands.borrow_mut().pop_front();
            let Some(cmd) = next_cmd else {
                break;
            };

            match cmd {
                PendingCommand::Send {
                    event_name,
                    payload_val,
                } => {
                    self.step_transition(&event_name, &payload_val)?;
                }
                PendingCommand::Stop => {
                    self.step_stop()?;
                }
            }
        }
        self.check_and_raise_error()
    }

    fn get_next_deadline(&self) -> PyResult<Option<u64>> {
        self.with_machine(|m| {
            if m.is_started
                && m.status != lavi::MachineStatus::Stopped
                && !m.is_done()
                && m.status != lavi::MachineStatus::Done
            {
                m.next_timer_deadline()
            } else {
                None
            }
        })
    }
}

#[pyclass(unsendable, name = "_ActionDecorator")]
pub struct ActionDecorator {
    machine: Py<Machine>,
    name: String,
}

#[pymethods]
impl ActionDecorator {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        func: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let machine_bound = self.machine.bind(py);
        Machine::register_action(machine_bound, &self.name, &func)?;
        Ok(func)
    }
}

#[pyclass(unsendable, name = "_GuardDecorator")]
pub struct GuardDecorator {
    machine: Py<Machine>,
    name: String,
}

#[pymethods]
impl GuardDecorator {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        func: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let machine_bound = self.machine.bind(py);
        Machine::register_guard(machine_bound, &self.name, &func)?;
        Ok(func)
    }
}

#[pyclass(unsendable, name = "_ClockDecorator")]
pub struct ClockDecorator {
    machine: Py<Machine>,
}

#[pymethods]
impl ClockDecorator {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        func: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let machine_bound = self.machine.bind(py);
        Machine::register_clock(machine_bound, &func)?;
        Ok(func)
    }
}

fn call_python_callback<'py>(
    py: Python<'py>,
    py_fn: &Bound<'py, PyAny>,
    ctx: PyObject,
    evt: PyObject,
    params: PyObject,
) -> PyResult<Bound<'py, PyAny>> {
    let (code_obj, is_bound) = if let Ok(code) = py_fn.getattr("__code__") {
        (Some(code), py_fn.hasattr("__self__").unwrap_or(false))
    } else if let Ok(f) = py_fn.getattr("__func__") {
        (f.getattr("__code__").ok(), true)
    } else if let Ok(call) = py_fn.getattr("__call__") {
        let is_method = call.hasattr("__self__").unwrap_or(false);
        (call.getattr("__code__").ok(), is_method)
    } else {
        (None, false)
    };

    let takes_three = if let Some(code) = code_obj {
        let argcount: usize = code.getattr("co_argcount").and_then(|a| a.extract()).unwrap_or(0);
        let flags: usize = code.getattr("co_flags").and_then(|f| f.extract()).unwrap_or(0);
        let has_varargs = (flags & 0x04) != 0;
        let effective_args = if is_bound && argcount > 0 { argcount - 1 } else { argcount };
        has_varargs || effective_args >= 3
    } else {
        false
    };

    if takes_three {
        py_fn.call1((ctx, evt, params))
    } else {
        match py_fn.call1((ctx.clone_ref(py), evt.clone_ref(py))) {
            Ok(res) => Ok(res),
            Err(e) => {
                if py_fn.getattr("__code__").is_err() {
                    if let Ok(res) = py_fn.call1((ctx, evt, params)) {
                        return Ok(res);
                    }
                }
                Err(e)
            }
        }
    }
}

#[pyclass(unsendable, weakref)]
pub struct Machine {
    state: Rc<MachineState>,
}

#[pymethods]
impl Machine {
    #[new]
    #[pyo3(signature = (json_config, clock=None))]
    fn new(json_config: &str, clock: Option<SimulatedClock>) -> PyResult<Self> {
        let def = lavi::from_json_str(json_config)
            .map_err(|e| PyValueError::new_err(format!("Invalid machine JSON: {e}")))?;
        let mut runtime_machine = lavi::RuntimeMachine::from_definition(def)
            .map_err(|e| PyValueError::new_err(format!("Invalid machine definition: {e}")))?;

        if let Some(ref c) = clock {
            let guard = c.inner.lock().unwrap();
            if guard.closed {
                return Err(PyRuntimeError::new_err("SimulatedClock is closed"));
            }
            runtime_machine.register_clock(guard.clock.as_clock_fn());
        }

        let state = Rc::new(MachineState {
            machine: RefCell::new(Some(runtime_machine)),
            active_machine: Cell::new(std::ptr::null()),
            callback_depth: Cell::new(0),
            aborted: Cell::new(false),
            pending_commands: RefCell::new(VecDeque::new()),
            python_actions: RefCell::new(IndexMap::new()),
            python_guards: RefCell::new(IndexMap::new()),
            python_clock: RefCell::new(None),
            guards: RefCell::new(Rc::new(IndexMap::new())),
            actions: RefCell::new(Rc::new(IndexMap::new())),
            clock: RefCell::new(clock.clone()),
            last_error: RefCell::new(None),
            is_closed: Cell::new(false),
        });

        if let Some(ref c) = clock {
            let mut guard = c.inner.lock().unwrap();
            let weak_state = Rc::downgrade(&state);
            if !guard.machines.iter().any(|w| w.ptr_eq(&weak_state)) {
                guard.machines.push(weak_state);
            }
        }

        Ok(Machine { state })
    }

    fn register_action<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        callback: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, Self>> {
        let machine_self = slf.borrow();
        if machine_self.state.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }

        let py_fn = callback.clone().unbind();
        machine_self
            .state
            .python_actions
            .borrow_mut()
            .insert(name.to_string(), py_fn);

        let weak_state = Rc::downgrade(&machine_self.state);
        let action_name = name.to_string();

        let action_fn: lavi::ActionFunction = alloc::rc::Rc::new(move |ctx, evt, params| {
            let Some(state) = weak_state.upgrade() else {
                return;
            };
            if state.aborted.get() {
                return;
            }

            let _guard = CallbackGuard::new(&state.callback_depth);
            let result = Python::with_gil(|py| -> PyResult<()> {
                let ctx_val = Value(Arc::new(ctx.clone())).into_py(py);
                let evt_val = Value(Arc::new(evt.clone())).into_py(py);
                let params_val = match params {
                    Some(p) => Value(Arc::new(p.clone())).into_py(py),
                    None => Value(Arc::new(lavi::Value::Null)).into_py(py),
                };
                let actions_map = state.python_actions.borrow();
                let Some(py_fn) = actions_map.get(&action_name) else {
                    return Ok(());
                };
                let bound = py_fn.bind(py);
                let _ = call_python_callback(py, bound, ctx_val, evt_val, params_val)?;
                Ok(())
            });

            if let Err(err) = result {
                state.aborted.set(true);
                *state.last_error.borrow_mut() = Some(err);
            }
        });

        let mut map = (**machine_self.state.actions.borrow()).clone();
        map.insert(name.to_string(), action_fn);
        *machine_self.state.actions.borrow_mut() = Rc::new(map);

        drop(machine_self);
        Ok(slf.clone())
    }

    fn register_guard<'py>(
        slf: &Bound<'py, Self>,
        name: &str,
        callback: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, Self>> {
        let machine_self = slf.borrow();
        if machine_self.state.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }

        let py_fn = callback.clone().unbind();
        machine_self
            .state
            .python_guards
            .borrow_mut()
            .insert(name.to_string(), py_fn);

        let weak_state = Rc::downgrade(&machine_self.state);
        let guard_name = name.to_string();

        let guard_fn: lavi::GuardFunction = alloc::rc::Rc::new(move |ctx, evt, params| {
            let Some(state) = weak_state.upgrade() else {
                return false;
            };
            if state.aborted.get() {
                return false;
            }

            let _guard = CallbackGuard::new(&state.callback_depth);
            let result = Python::with_gil(|py| -> PyResult<bool> {
                let ctx_val = Value(Arc::new(ctx.clone())).into_py(py);
                let evt_val = Value(Arc::new(evt.clone())).into_py(py);
                let params_val = match params {
                    Some(p) => Value(Arc::new(p.clone())).into_py(py),
                    None => Value(Arc::new(lavi::Value::Null)).into_py(py),
                };
                let guards_map = state.python_guards.borrow();
                let Some(py_fn) = guards_map.get(&guard_name) else {
                    return Ok(false);
                };
                let bound = py_fn.bind(py);
                let ret = call_python_callback(py, bound, ctx_val, evt_val, params_val)?;
                ret.is_truthy()
            });

            match result {
                Ok(b) => b,
                Err(err) => {
                    state.aborted.set(true);
                    *state.last_error.borrow_mut() = Some(err);
                    false
                }
            }
        });

        let mut map = (**machine_self.state.guards.borrow()).clone();
        map.insert(name.to_string(), guard_fn);
        *machine_self.state.guards.borrow_mut() = Rc::new(map);

        drop(machine_self);
        Ok(slf.clone())
    }

    fn register_clock<'py>(
        slf: &Bound<'py, Self>,
        callback: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, Self>> {
        let machine_self = slf.borrow();
        if machine_self.state.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }

        if let Ok(sim_clock) = callback.extract::<PyRef<SimulatedClock>>() {
            let clock_clone = (*sim_clock).clone();
            let guard = clock_clone.inner.lock().unwrap();
            if guard.closed {
                return Err(PyRuntimeError::new_err("SimulatedClock is closed"));
            }
            let clock_fn = guard.clock.as_clock_fn();
            drop(guard);

            if let Some(ref mut m) = *machine_self.state.machine.borrow_mut() {
                m.register_clock(clock_fn);
            }
            *machine_self.state.clock.borrow_mut() = Some(clock_clone.clone());
            let mut guard = clock_clone.inner.lock().unwrap();
            let weak_state = Rc::downgrade(&machine_self.state);
            if !guard.machines.iter().any(|w| w.ptr_eq(&weak_state)) {
                guard.machines.push(weak_state);
            }
        } else if callback.is_callable() {
            let py_fn = callback.clone().unbind();
            *machine_self.state.python_clock.borrow_mut() = Some(py_fn);
            let weak_state = Rc::downgrade(&machine_self.state);
            let clock_fn: alloc::rc::Rc<dyn Fn() -> u64> = alloc::rc::Rc::new(move || {
                let Some(state) = weak_state.upgrade() else {
                    return 0;
                };
                if state.aborted.get() {
                    return 0;
                }
                Python::with_gil(|py| {
                    let clock_opt = state.python_clock.borrow();
                    if let Some(ref py_fn) = *clock_opt {
                        match py_fn.bind(py).call0() {
                            Ok(res) => match res.extract::<u64>() {
                                Ok(ms) => return ms,
                                Err(err) => {
                                    state.aborted.set(true);
                                    *state.last_error.borrow_mut() = Some(err);
                                }
                            },
                            Err(err) => {
                                state.aborted.set(true);
                                *state.last_error.borrow_mut() = Some(err);
                            }
                        }
                    }
                    0
                })
            });
            if let Some(ref mut m) = *machine_self.state.machine.borrow_mut() {
                m.register_clock(clock_fn);
            }
        } else {
            return Err(PyTypeError::new_err(
                "clock must be a SimulatedClock or callable returning int",
            ));
        }

        drop(machine_self);
        Ok(slf.clone())
    }

    #[pyo3(signature = (name))]
    fn action<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<ActionDecorator> {
        Ok(ActionDecorator {
            machine: slf.clone().unbind(),
            name: name.to_string(),
        })
    }

    #[pyo3(signature = (name))]
    fn guard<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<GuardDecorator> {
        Ok(GuardDecorator {
            machine: slf.clone().unbind(),
            name: name.to_string(),
        })
    }

    #[pyo3(signature = (callback=None))]
    fn clock<'py>(
        slf: &Bound<'py, Self>,
        callback: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        match callback {
            Some(cb) => {
                Self::register_clock(slf, cb)?;
                Ok(cb.clone())
            }
            None => {
                let decorator = ClockDecorator {
                    machine: slf.clone().unbind(),
                };
                Ok(Bound::new(py, decorator)?.into_any())
            }
        }
    }

    fn start<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, Self>> {
        let machine_self = slf.borrow();
        machine_self.state.step_start()?;
        drop(machine_self);
        Ok(slf.clone())
    }

    fn stop(&self) -> PyResult<()> {
        if self.state.is_in_callback() {
            self.state
                .pending_commands
                .borrow_mut()
                .push_back(PendingCommand::Stop);
            return Ok(());
        }
        self.state.step_stop()
    }

    fn close(&self) -> PyResult<()> {
        self.state.is_closed.set(true);
        *self.state.machine.borrow_mut() = None;
        self.state.python_actions.borrow_mut().clear();
        self.state.python_guards.borrow_mut().clear();
        *self.state.python_clock.borrow_mut() = None;
        *self.state.guards.borrow_mut() = Rc::new(IndexMap::new());
        *self.state.actions.borrow_mut() = Rc::new(IndexMap::new());
        self.state.pending_commands.borrow_mut().clear();
        Ok(())
    }

    fn matches(&self, state_path: &str) -> PyResult<bool> {
        self.state.with_machine(|m| {
            if !m.is_started {
                false
            } else {
                m.matches(state_path)
            }
        })
    }

    fn get_context(&self) -> PyResult<Value> {
        self.state.with_machine(|m| {
            if !m.is_started {
                Value(Arc::new(lavi::Value::Null))
            } else {
                Value(Arc::new(m.context.clone()))
            }
        })
    }

    #[getter(context)]
    fn py_context(&self) -> PyResult<Value> {
        self.get_context()
    }

    #[pyo3(signature = (event, payload=None))]
    fn can(&self, event: &str, payload: Option<&Bound<'_, PyAny>>) -> PyResult<bool> {
        let payload_val = match payload {
            None => lavi::Value::event(event),
            Some(p) => {
                if let Ok(json_str) = p.extract::<String>() {
                    let parsed = lavi::Value::from_json(&json_str)
                        .map_err(|e| PyValueError::new_err(format!("Invalid payload JSON: {e:?}")))?;
                    let mut obj = parsed.into_object().unwrap_or_default();
                    if !obj.contains_key("type") {
                        obj.insert(
                            "type".to_string(),
                            lavi::Value::String(event.to_string()),
                        );
                    }
                    lavi::Value::Object(obj)
                } else {
                    let core_val = python_to_core_value(p)?;
                    let mut obj = core_val.into_object().unwrap_or_default();
                    if !obj.contains_key("type") {
                        obj.insert(
                            "type".to_string(),
                            lavi::Value::String(event.to_string()),
                        );
                    }
                    lavi::Value::Object(obj)
                }
            }
        };

        let guards = self.state.guards.borrow().clone();
        self.state.with_machine(|m| {
            if !m.is_started || m.status == lavi::MachineStatus::Stopped {
                false
            } else {
                let prev_active = self.state.active_machine.get();
                if prev_active.is_null() {
                    self.state
                        .active_machine
                        .set(m as *const lavi::RuntimeMachine);
                }
                let res = m.can_with_guards(event, &payload_val, &guards);
                if prev_active.is_null() {
                    self.state.active_machine.set(std::ptr::null());
                }
                res
            }
        })
    }

    fn has_tag(&self, tag: &str) -> PyResult<bool> {
        self.state.with_machine(|m| {
            if !m.is_started {
                false
            } else {
                m.has_tag(tag)
            }
        })
    }

    fn is_done(&self) -> PyResult<bool> {
        self.state.with_machine(|m| {
            if !m.is_started {
                false
            } else {
                m.is_done()
            }
        })
    }

    #[pyo3(signature = (event, payload=None))]
    fn send(
        &self,
        event: &Bound<'_, PyAny>,
        payload: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if self.state.is_closed.get() {
            return Err(PyRuntimeError::new_err("Machine is closed"));
        }

        let is_running = self.state.with_machine(|m| {
            m.is_started && m.status != lavi::MachineStatus::Stopped
        })?;

        if !is_running {
            return Ok(());
        }

        let (event_name, payload_val) = if let Ok(s) = event.extract::<String>() {
            let p_val = match payload {
                None => lavi::Value::event(&s),
                Some(p) => {
                    if let Ok(json_str) = p.extract::<String>() {
                        let parsed = lavi::Value::from_json(&json_str).map_err(|e| {
                            PyValueError::new_err(format!("Invalid payload JSON: {e:?}"))
                        })?;
                        let mut obj = parsed.into_object().unwrap_or_default();
                        if !obj.contains_key("type") {
                            obj.insert("type".to_string(), lavi::Value::String(s.clone()));
                        }
                        lavi::Value::Object(obj)
                    } else {
                        let core_val = python_to_core_value(p)?;
                        let mut obj = core_val.into_object().unwrap_or_default();
                        if !obj.contains_key("type") {
                            obj.insert("type".to_string(), lavi::Value::String(s.clone()));
                        }
                        lavi::Value::Object(obj)
                    }
                }
            };
            (s, p_val)
        } else if let Ok(dict) = event.downcast::<PyDict>() {
            let type_item = dict
                .get_item("type")?
                .ok_or_else(|| PyValueError::new_err("Event dict must contain a 'type' field"))?;
            let event_name = type_item.extract::<String>()?;
            let core_val = python_to_core_value(event)?;
            let mut obj = core_val.into_object().unwrap_or_default();
            if let Some(p) = payload {
                if let Ok(json_str) = p.extract::<String>() {
                    let parsed = lavi::Value::from_json(&json_str).map_err(|e| {
                        PyValueError::new_err(format!("Invalid payload JSON: {e:?}"))
                    })?;
                    if let Some(extra) = parsed.into_object() {
                        for (k, v) in extra {
                            obj.insert(k, v);
                        }
                    }
                } else {
                    let extra = python_to_core_value(p)?;
                    if let Some(extra_map) = extra.into_object() {
                        for (k, v) in extra_map {
                            obj.insert(k, v);
                        }
                    }
                }
            }
            (event_name, lavi::Value::Object(obj))
        } else {
            return Err(PyTypeError::new_err("Event must be a string or a dict"));
        };

        if self.state.is_in_callback() {
            self.state
                .pending_commands
                .borrow_mut()
                .push_back(PendingCommand::Send {
                    event_name,
                    payload_val,
                });
            return Ok(());
        }

        self.state.step_transition(&event_name, &payload_val)
    }

    fn tick(&self) -> PyResult<()> {
        self.state.step_tick()
    }

    fn get_next_deadline(&self) -> PyResult<Option<u64>> {
        self.state.get_next_deadline()
    }

    fn get_snapshot(&self) -> PyResult<String> {
        self.state.with_machine(|m: &lavi::RuntimeMachine| m.get_persisted_snapshot_json())
    }

    fn restore_snapshot(&self, snapshot_json: &str) -> PyResult<()> {
        let mut guard = self.state.machine.borrow_mut();
        let inner = guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        inner
            .restore_from_snapshot_json(snapshot_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid snapshot: {e}")))?;
        Ok(())
    }

    fn __enter__<'py>(slf: &Bound<'py, Self>) -> Bound<'py, Self> {
        slf.clone()
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close()
    }

    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        let actions = self.state.python_actions.borrow();
        for py_fn in actions.values() {
            visit.call(py_fn)?;
        }
        let guards = self.state.python_guards.borrow();
        for py_fn in guards.values() {
            visit.call(py_fn)?;
        }
        let clock = self.state.python_clock.borrow();
        if let Some(ref py_fn) = *clock {
            visit.call(py_fn)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.state.python_actions.borrow_mut().clear();
        self.state.python_guards.borrow_mut().clear();
        *self.state.python_clock.borrow_mut() = None;
        *self.state.guards.borrow_mut() = Rc::new(IndexMap::new());
        *self.state.actions.borrow_mut() = Rc::new(IndexMap::new());
        self.state.pending_commands.borrow_mut().clear();
    }
}

// ---------------------------------------------------------------------------
// Python Module Definition
// ---------------------------------------------------------------------------

#[pymodule]
fn _lavi_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Value>()?;
    m.add_class::<SimulatedClock>()?;
    m.add_class::<Machine>()?;
    m.add_class::<ActionDecorator>()?;
    m.add_class::<GuardDecorator>()?;
    m.add_class::<ClockDecorator>()?;
    Ok(())
}
