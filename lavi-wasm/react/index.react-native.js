'use strict';

const { createActor } = require('../react-native/lavi.js');
const { createReactHooks } = require('./core.cjs');

module.exports = createReactHooks(createActor);
