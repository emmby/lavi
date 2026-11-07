export { cancel, enqueueActions, log, raise, and, or, not, stateIn } from '../lavi-wasm/pkg-npm/lavi';
export const sendParent = () => {};
export const sendTo = () => {};
export const spawnChild = () => {};
export const stopChild = () => {};
export const CallbackActorRef = {};
export const fromCallback = (fn?: any) => fn;
export const forwardTo = () => {};
export const fromPromise = (fn?: any) => fn;
export const fromObservable = (fn?: any) => fn;
export const fromTransition = (fn?: any, initial?: any) => fn;
export const of = (...args: any[]) => args;
export const toPromise = (actor?: any) => Promise.resolve(actor);
export const createDoneActorEvent = (id: string, output?: any) => ({ type: `xstate.done.actor.${id}`, output });
export const resolveReferencedActor = (actor?: any) => actor;

// Mocked types for TS:
export type ActorRef = any;
export type ActorRefFromLogic = any;
export type AnyActorRef = any;
export type EventObject = any;
export type Snapshot = any;
export type EventFrom<T = any> = any;
export type ExecutableActionsFrom<T = any> = any;
export type ExecutableSpawnAction = any;
