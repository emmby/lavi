'use strict';

const { createActor } = require('../node/lavi.js');
const { createReactHooks } = require('./core.cjs');

module.exports = createReactHooks(createActor);
