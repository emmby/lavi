import type { Actor, Snapshot, StateNode } from '../node/lavi.js';

export type SnapshotFrom<T> = T extends { getSnapshot(): infer S }
  ? S
  : Snapshot;

export type ObserverOrListener<TSnapshot = Snapshot> =
  | ((snapshot: TSnapshot) => void)
  | {
      next?: (snapshot: TSnapshot) => void;
      error?: (error: any) => void;
      complete?: () => void;
    };

export type ActorSnapshot<TActor> = [TActor] extends [undefined | null]
  ? undefined
  : SnapshotFrom<NonNullable<TActor>> | (undefined extends TActor ? undefined : never) | (null extends TActor ? undefined : never);

export function useSelector<
  TActor extends { getSnapshot(): any; subscribe(observer: any): any } | undefined | null =
    | Actor
    | undefined,
  TSelected = ActorSnapshot<TActor>,
>(
  actorRef: TActor,
  selector?: (snapshot: ActorSnapshot<TActor>) => TSelected,
  compare?: (a: TSelected, b: TSelected) => boolean
): TSelected;

export function useActorRef<TMachine = StateNode | Record<string, any>>(
  machineOrLogic: TMachine | (() => TMachine),
  options?: Record<string, any>,
  observerOrListener?: ObserverOrListener<Snapshot>
): Actor;

export function useActor<TMachine = StateNode | Record<string, any>>(
  machineOrLogic: TMachine | (() => TMachine),
  options?: Record<string, any>
): [Snapshot, (event: any) => any, Actor];

export function useMachine<TMachine = StateNode | Record<string, any>>(
  machineOrLogic: TMachine | (() => TMachine),
  options?: Record<string, any>
): [Snapshot, (event: any) => any, Actor];
