import { createActor } from '../bundler/lavi.js';
import core from './core.cjs';

const { useSelector, useActorRef, useActor, useMachine } =
  core.createReactHooks(createActor);

export { useSelector, useActorRef, useActor, useMachine };
