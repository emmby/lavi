import 'dart:ffi';
import 'package:ffi/ffi.dart';
import 'loader.dart';

final class NativeMachineContext extends Opaque {}

final class NativeSimulatedClock extends Opaque {}

final class NativeFsmValue extends Opaque {}

typedef NativeActionCallback = Void Function(
  Pointer<Utf8> actionName,
  Pointer<NativeFsmValue> context,
  Pointer<NativeFsmValue> event,
  Pointer<Void> userData,
);

typedef NativeActionWithParamsCallback = Void Function(
  Pointer<Utf8> actionName,
  Pointer<NativeFsmValue> context,
  Pointer<NativeFsmValue> event,
  Pointer<NativeFsmValue> params,
  Pointer<Void> userData,
);

typedef NativeGuardCallback = Bool Function(
  Pointer<Utf8> guardName,
  Pointer<NativeFsmValue> context,
  Pointer<NativeFsmValue> event,
  Pointer<Void> userData,
);

typedef NativeGuardWithParamsCallback = Bool Function(
  Pointer<Utf8> guardName,
  Pointer<NativeFsmValue> context,
  Pointer<NativeFsmValue> event,
  Pointer<NativeFsmValue> params,
  Pointer<Void> userData,
);

typedef NativeDelayCallback = Uint64 Function(
  Pointer<Utf8> delayName,
  Pointer<NativeFsmValue> context,
  Pointer<NativeFsmValue> event,
  Pointer<Void> userData,
);

typedef NativeClockWithDataCallback = Uint64 Function(
  Pointer<Void> userData,
);

class LaviBindings {
  final DynamicLibrary dylib;

  LaviBindings(this.dylib);

  late final fsmCreate = dylib.lookupFunction<
      Pointer<NativeMachineContext> Function(Pointer<Utf8>),
      Pointer<NativeMachineContext> Function(Pointer<Utf8>)>('fsm_create');

  late final fsmCreateWithInput = dylib.lookupFunction<
      Pointer<NativeMachineContext> Function(Pointer<Utf8>, Pointer<Utf8>),
      Pointer<NativeMachineContext> Function(
          Pointer<Utf8>, Pointer<Utf8>)>('fsm_create_with_input');

  late final fsmStart = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>),
      int Function(Pointer<NativeMachineContext>)>('fsm_start');

  late final fsmFree = dylib.lookupFunction<
      Void Function(Pointer<NativeMachineContext>),
      void Function(Pointer<NativeMachineContext>)>('fsm_free');

  late final fsmMatches = dylib.lookupFunction<
      Bool Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      bool Function(Pointer<NativeMachineContext>, Pointer<Utf8>)>('fsm_matches');

  late final fsmCan = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>)>('fsm_can');

  late final fsmHasTag = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>)>('fsm_has_tag');

  late final fsmIsDone = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>),
      int Function(Pointer<NativeMachineContext>)>('fsm_is_done');

  late final fsmStop = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>),
      int Function(Pointer<NativeMachineContext>)>('fsm_stop');

  late final fsmSendEvent = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>)>('fsm_send_event');

  late final fsmSendJsonEvent = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      int Function(
          Pointer<NativeMachineContext>, Pointer<Utf8>)>('fsm_send_json_event');

  late final fsmGetState = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Uint8>, Size),
      int Function(
          Pointer<NativeMachineContext>, Pointer<Uint8>, int)>('fsm_get_state');

  late final fsmValueIsObject = dylib.lookupFunction<
      Bool Function(Pointer<NativeFsmValue>),
      bool Function(Pointer<NativeFsmValue>)>('fsm_value_is_object');

  late final fsmValueIsString = dylib.lookupFunction<
      Bool Function(Pointer<NativeFsmValue>),
      bool Function(Pointer<NativeFsmValue>)>('fsm_value_is_string');

  late final fsmValueIsNumber = dylib.lookupFunction<
      Bool Function(Pointer<NativeFsmValue>),
      bool Function(Pointer<NativeFsmValue>)>('fsm_value_is_number');

  late final fsmValueIsBool = dylib.lookupFunction<
      Bool Function(Pointer<NativeFsmValue>),
      bool Function(Pointer<NativeFsmValue>)>('fsm_value_is_bool');

  late final fsmValueGetString = dylib.lookupFunction<
      Size Function(Pointer<NativeFsmValue>, Pointer<Uint8>, Size),
      int Function(
          Pointer<NativeFsmValue>, Pointer<Uint8>, int)>('fsm_value_get_string');

  late final fsmValueGetNumber = dylib.lookupFunction<
      Double Function(Pointer<NativeFsmValue>),
      double Function(Pointer<NativeFsmValue>)>('fsm_value_get_number');

  late final fsmValueGetBool = dylib.lookupFunction<
      Bool Function(Pointer<NativeFsmValue>),
      bool Function(Pointer<NativeFsmValue>)>('fsm_value_get_bool');

  late final fsmValueGetProperty = dylib.lookupFunction<
      Pointer<NativeFsmValue> Function(Pointer<NativeFsmValue>, Pointer<Utf8>),
      Pointer<NativeFsmValue> Function(
          Pointer<NativeFsmValue>, Pointer<Utf8>)>('fsm_value_get_property');

  late final fsmRegisterAction = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeActionCallback>>, Pointer<Void>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeActionCallback>>, Pointer<Void>)>(
    'fsm_register_action',
  );

  late final fsmRegisterActionWithParams = dylib.lookupFunction<
      Int32 Function(
          Pointer<NativeMachineContext>,
          Pointer<Utf8>,
          Pointer<NativeFunction<NativeActionWithParamsCallback>>,
          Pointer<Void>),
      int Function(
          Pointer<NativeMachineContext>,
          Pointer<Utf8>,
          Pointer<NativeFunction<NativeActionWithParamsCallback>>,
          Pointer<Void>)>(
    'fsm_register_action_with_params',
  );

  late final fsmRegisterGuard = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeGuardCallback>>, Pointer<Void>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeGuardCallback>>, Pointer<Void>)>(
    'fsm_register_guard',
  );

  late final fsmRegisterGuardWithParams = dylib.lookupFunction<
      Int32 Function(
          Pointer<NativeMachineContext>,
          Pointer<Utf8>,
          Pointer<NativeFunction<NativeGuardWithParamsCallback>>,
          Pointer<Void>),
      int Function(
          Pointer<NativeMachineContext>,
          Pointer<Utf8>,
          Pointer<NativeFunction<NativeGuardWithParamsCallback>>,
          Pointer<Void>)>(
    'fsm_register_guard_with_params',
  );

  late final fsmRegisterDelay = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeDelayCallback>>, Pointer<Void>),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>,
          Pointer<NativeFunction<NativeDelayCallback>>, Pointer<Void>)>(
    'fsm_register_delay',
  );

  late final fsmRegisterClockWithData = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>,
          Pointer<NativeFunction<NativeClockWithDataCallback>>, Pointer<Void>),
      int Function(Pointer<NativeMachineContext>,
          Pointer<NativeFunction<NativeClockWithDataCallback>>, Pointer<Void>)>(
    'fsm_register_clock_with_data',
  );

  late final fsmGetNextDeadline = dylib.lookupFunction<
      Bool Function(Pointer<NativeMachineContext>, Pointer<Uint64>),
      bool Function(
          Pointer<NativeMachineContext>, Pointer<Uint64>)>('fsm_get_next_deadline');

  late final fsmSimulatedClockCreate = dylib.lookupFunction<
      Pointer<NativeSimulatedClock> Function(Uint64),
      Pointer<NativeSimulatedClock> Function(int)>('fsm_simulated_clock_create');

  late final fsmSimulatedClockFree = dylib.lookupFunction<
      Void Function(Pointer<NativeSimulatedClock>),
      void Function(Pointer<NativeSimulatedClock>)>('fsm_simulated_clock_free');

  late final fsmRegisterSimulatedClock = dylib.lookupFunction<
      Int32 Function(
          Pointer<NativeMachineContext>, Pointer<NativeSimulatedClock>),
      int Function(Pointer<NativeMachineContext>,
          Pointer<NativeSimulatedClock>)>('fsm_register_simulated_clock');

  late final fsmSimulatedClockNow = dylib.lookupFunction<
      Uint64 Function(Pointer<NativeSimulatedClock>),
      int Function(Pointer<NativeSimulatedClock>)>('fsm_simulated_clock_now');

  late final fsmSimulatedClockSet = dylib.lookupFunction<
      Int32 Function(Pointer<NativeSimulatedClock>, Uint64),
      int Function(
          Pointer<NativeSimulatedClock>, int)>('fsm_simulated_clock_set');

  late final fsmSimulatedClockIncrement = dylib.lookupFunction<
      Int32 Function(Pointer<NativeSimulatedClock>, Uint64),
      int Function(
          Pointer<NativeSimulatedClock>, int)>('fsm_simulated_clock_increment');

  late final fsmTick = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>),
      int Function(Pointer<NativeMachineContext>)>('fsm_tick');

  late final fsmGetSnapshotJson = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Uint8>, Size),
      int Function(Pointer<NativeMachineContext>, Pointer<Uint8>,
          int)>('fsm_get_snapshot_json');

  late final fsmRestoreSnapshotJson = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Utf8>),
      int Function(Pointer<NativeMachineContext>,
          Pointer<Utf8>)>('fsm_restore_snapshot_json');

  late final fsmGetOutputJson = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Uint8>, Size),
      int Function(Pointer<NativeMachineContext>, Pointer<Uint8>,
          int)>('fsm_get_output_json');

  late final fsmGetMetaJson = dylib.lookupFunction<
      Int32 Function(Pointer<NativeMachineContext>, Pointer<Uint8>, Size),
      int Function(
          Pointer<NativeMachineContext>, Pointer<Uint8>, int)>('fsm_get_meta_json');

  late final fsmGetStateMetaJson = dylib.lookupFunction<
      Int32 Function(
          Pointer<NativeMachineContext>, Pointer<Utf8>, Pointer<Uint8>, Size),
      int Function(Pointer<NativeMachineContext>, Pointer<Utf8>, Pointer<Uint8>,
          int)>('fsm_get_state_meta_json');
}

LaviBindings? _cachedBindings;

LaviBindings getBindings() {
  return _cachedBindings ??= LaviBindings(loadLaviLibrary());
}
