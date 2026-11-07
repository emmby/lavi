#![allow(clippy::useless_conversion)]
#![allow(clippy::arc_with_non_send_sync)]

use indexmap::IndexMap;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::cell::RefCell;
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
                let actual_idx = if idx < 0 {
                    (arr.len() as isize + idx) as usize
                } else {
                    idx as usize
                };
                if actual_idx < arr.len() {
                    return Value(Arc::new(arr[actual_idx].clone()));
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
    pub machines: Vec<Weak<RefCell<Option<MachineInner>>>>,
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
                let current: Vec<Rc<RefCell<Option<MachineInner>>>> =
                    guard.machines.iter().filter_map(|w| w.upgrade()).collect();
                (guard.clock.clone(), current)
            };

            let mut min_deadline: Option<u64> = None;
            for m_rc in &machines_to_step {
                let inner_borrow = m_rc.borrow();
                if let Some(inner) = inner_borrow.as_ref() {
                    if inner.machine.is_started
                        && inner.machine.status != lavi::MachineStatus::Stopped
                        && !inner.machine.is_done()
                        && inner.machine.status != lavi::MachineStatus::Done
                    {
                        if let Some(d) = inner.machine.next_timer_deadline() {
                            min_deadline = Some(min_deadline.map_or(d, |curr| curr.min(d)));
                        }
                    }
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

            for m_rc in &machines_to_step {
                let mut inner_borrow = m_rc.borrow_mut();
                if let Some(inner) = inner_borrow.as_mut() {
                    if inner.machine.is_started
                        && inner.machine.status != lavi::MachineStatus::Stopped
                        && !inner.machine.is_done()
                        && inner.machine.status != lavi::MachineStatus::Done
                    {
                        let _ = inner.machine.tick(&inner.guards, &inner.actions);
                    }
                }
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
}

// ---------------------------------------------------------------------------
// Machine pyclass
// ---------------------------------------------------------------------------

pub struct MachineInner {
    pub machine: lavi::RuntimeMachine,
    pub guards: IndexMap<String, lavi::GuardFunction>,
    pub actions: IndexMap<String, lavi::ActionFunction>,
    pub clock: Option<SimulatedClock>,
}

#[pyclass(unsendable)]
pub struct Machine {
    inner: Rc<RefCell<Option<MachineInner>>>,
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

        let inner = Rc::new(RefCell::new(Some(MachineInner {
            machine: runtime_machine,
            guards: IndexMap::new(),
            actions: IndexMap::new(),
            clock: clock.clone(),
        })));

        if let Some(ref c) = clock {
            let mut guard = c.inner.lock().unwrap();
            guard.machines.push(Rc::downgrade(&inner));
        }

        Ok(Machine { inner })
    }

    fn start<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, Self>> {
        let machine_self = slf.borrow();
        let mut inner_guard = machine_self.inner.borrow_mut();
        let inner = inner_guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started {
            inner.machine.start(&inner.guards, &inner.actions);
        }
        drop(inner_guard);
        drop(machine_self);
        Ok(slf.clone())
    }

    fn stop(&self) -> PyResult<()> {
        let mut inner_guard = self.inner.borrow_mut();
        let inner = inner_guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        inner.machine.stop();
        Ok(())
    }

    fn close(&self) -> PyResult<()> {
        let mut inner_guard = self.inner.borrow_mut();
        *inner_guard = None;
        Ok(())
    }

    fn matches(&self, state_path: &str) -> PyResult<bool> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started {
            return Ok(false);
        }
        Ok(inner.machine.matches(state_path))
    }

    #[pyo3(signature = (event, payload=None))]
    fn can(&self, event: &str, payload: Option<&Bound<'_, PyAny>>) -> PyResult<bool> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started || inner.machine.status == lavi::MachineStatus::Stopped {
            return Ok(false);
        }

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

        Ok(inner
            .machine
            .can_with_guards(event, &payload_val, &inner.guards))
    }

    fn has_tag(&self, tag: &str) -> PyResult<bool> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started {
            return Ok(false);
        }
        Ok(inner.machine.has_tag(tag))
    }

    fn is_done(&self) -> PyResult<bool> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started {
            return Ok(false);
        }
        Ok(inner.machine.is_done())
    }

    #[pyo3(signature = (event, payload=None))]
    fn send(
        &self,
        event: &Bound<'_, PyAny>,
        payload: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let mut inner_guard = self.inner.borrow_mut();
        let inner = inner_guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started || inner.machine.status == lavi::MachineStatus::Stopped {
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

        inner
            .machine
            .transition(&event_name, &payload_val, &inner.guards, &inner.actions);
        Ok(())
    }

    fn tick(&self) -> PyResult<()> {
        let mut inner_guard = self.inner.borrow_mut();
        let inner = inner_guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if inner.machine.is_started && inner.machine.status != lavi::MachineStatus::Stopped {
            let _ = inner.machine.tick(&inner.guards, &inner.actions);
        }
        Ok(())
    }

    fn get_next_deadline(&self) -> PyResult<Option<u64>> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        if !inner.machine.is_started {
            return Ok(None);
        }
        Ok(inner.machine.next_timer_deadline())
    }

    fn get_snapshot(&self) -> PyResult<String> {
        let inner_guard = self.inner.borrow();
        let inner = inner_guard
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        Ok(inner.machine.get_persisted_snapshot_json())
    }

    fn restore_snapshot(&self, snapshot_json: &str) -> PyResult<()> {
        let mut inner_guard = self.inner.borrow_mut();
        let inner = inner_guard
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("Machine is closed"))?;
        inner
            .machine
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
}

// ---------------------------------------------------------------------------
// Python Module Definition
// ---------------------------------------------------------------------------

#[pymodule]
fn _lavi_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Value>()?;
    m.add_class::<SimulatedClock>()?;
    m.add_class::<Machine>()?;
    Ok(())
}
