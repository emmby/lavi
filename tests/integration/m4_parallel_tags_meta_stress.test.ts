import { describe, it, expect } from 'vitest';
import {
  createMachine,
  createActor,
  initialTransition,
  transition
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 4 Iteration 3: Parallel Tags & Meta Stress Suite', () => {
  const complexParallelConfig = {
    id: 'deepParallelSystem',
    type: 'parallel',
    tags: ['system_active', 'shared_tag'],
    meta: { level: 'root', version: '2.0' },
    states: {
      audioRegion: {
        tags: ['audio_subsystem', 'shared_tag'],
        meta: { region: 'audio' },
        initial: 'volumeControl',
        states: {
          volumeControl: {
            tags: ['vol_control'],
            meta: { module: 'volume' },
            initial: 'muted',
            states: {
              muted: {
                id: 'custom_audio_muted_id',
                tags: ['muted_state'],
                meta: { volumeLevel: 0, state: 'silent' },
                on: {
                  UNMUTE: 'unmuted'
                }
              },
              unmuted: {
                tags: ['sound_playing'],
                meta: { volumeLevel: 50, state: 'audible' },
                on: {
                  MUTE: 'muted'
                }
              }
            }
          }
        }
      },
      videoRegion: {
        id: 'custom_video_region_id',
        tags: ['video_subsystem'],
        meta: { region: 'video' },
        initial: 'idle',
        states: {
          idle: {
            tags: ['video_idle', 'shared_tag'],
            meta: { pipeline: 'stopped' },
            on: {
              PLAY: 'playing'
            }
          },
          playing: {
            tags: ['video_playing', 'hd_active'],
            meta: { pipeline: 'streaming', resolution: '4K' },
            on: {
              STOP: 'idle'
            }
          }
        }
      },
      networkRegion: {
        type: 'parallel',
        tags: ['network_subsystem'],
        meta: { region: 'network_parallel' },
        states: {
          wifi: {
            tags: ['wifi_module'],
            meta: { iface: 'wlan0' },
            initial: 'connected',
            states: {
              connected: {
                tags: ['wifi_online'],
                meta: { ssid: 'OfficeMesh', rssi: -45 },
                on: {
                  DISCONNECT_WIFI: 'disconnected'
                }
              },
              disconnected: {
                tags: ['wifi_offline'],
                meta: { ssid: null }
              }
            }
          },
          bluetooth: {
            tags: ['bt_module'],
            meta: { iface: 'bt0' },
            initial: 'scanning',
            states: {
              scanning: {
                tags: ['bt_scan'],
                meta: { intervalMs: 250 },
                on: {
                  PAIR: 'paired'
                }
              },
              paired: {
                id: 'custom_bt_paired_id',
                tags: ['bt_connected'],
                meta: { deviceName: 'Headphones' }
              }
            }
          }
        }
      }
    }
  };

  it('1.1 correctly aggregates tags and metadata across all 3 orthogonal regions on start', () => {
    const actor = createActor(createMachine(complexParallelConfig)).start();
    const snap = actor.getSnapshot();

    // Verify snapshot state value
    expect(snap.value).toEqual({
      audioRegion: { volumeControl: 'muted' },
      videoRegion: 'idle',
      networkRegion: {
        wifi: 'connected',
        bluetooth: 'scanning'
      }
    });

    // Verify tags set semantics
    expect(snap.tags).toBeInstanceOf(Set);
    expect(Array.from(snap.tags).sort()).toEqual([
      'audio_subsystem',
      'bt_module',
      'bt_scan',
      'muted_state',
      'network_subsystem',
      'shared_tag',
      'system_active',
      'video_idle',
      'video_subsystem',
      'vol_control',
      'wifi_module',
      'wifi_online'
    ].sort());

    // Verify hasTag() for all active tags
    expect(snap.hasTag('system_active')).toBe(true);
    expect(snap.hasTag('shared_tag')).toBe(true);
    expect(snap.hasTag('audio_subsystem')).toBe(true);
    expect(snap.hasTag('vol_control')).toBe(true);
    expect(snap.hasTag('muted_state')).toBe(true);
    expect(snap.hasTag('video_subsystem')).toBe(true);
    expect(snap.hasTag('video_idle')).toBe(true);
    expect(snap.hasTag('network_subsystem')).toBe(true);
    expect(snap.hasTag('wifi_module')).toBe(true);
    expect(snap.hasTag('wifi_online')).toBe(true);
    expect(snap.hasTag('bt_module')).toBe(true);
    expect(snap.hasTag('bt_scan')).toBe(true);

    // Verify inactive tags are absent
    expect(snap.hasTag('sound_playing')).toBe(false);
    expect(snap.hasTag('video_playing')).toBe(false);
    expect(snap.hasTag('hd_active')).toBe(false);
    expect(snap.hasTag('wifi_offline')).toBe(false);
    expect(snap.hasTag('bt_connected')).toBe(false);
    expect(snap.hasTag('unknown_tag')).toBe(false);

    // Verify metadata aggregation
    const meta = snap.getMeta();
    expect(meta).toEqual({
      deepParallelSystem: { level: 'root', version: '2.0' },
      'deepParallelSystem.audioRegion': { region: 'audio' },
      'deepParallelSystem.audioRegion.volumeControl': { module: 'volume' },
      custom_audio_muted_id: { volumeLevel: 0, state: 'silent' },
      custom_video_region_id: { region: 'video' },
      'deepParallelSystem.videoRegion.idle': { pipeline: 'stopped' },
      'deepParallelSystem.networkRegion': { region: 'network_parallel' },
      'deepParallelSystem.networkRegion.wifi': { iface: 'wlan0' },
      'deepParallelSystem.networkRegion.wifi.connected': { ssid: 'OfficeMesh', rssi: -45 },
      'deepParallelSystem.networkRegion.bluetooth': { iface: 'bt0' },
      'deepParallelSystem.networkRegion.bluetooth.scanning': { intervalMs: 250 }
    });
  });

  it('1.2 updates tags and metadata cleanly when single orthogonal region transitions', () => {
    const actor = createActor(createMachine(complexParallelConfig)).start();

    // Transition only audioRegion: UNMUTE
    actor.send({ type: 'UNMUTE' });
    const snap = actor.getSnapshot();

    expect(snap.value).toEqual({
      audioRegion: { volumeControl: 'unmuted' },
      videoRegion: 'idle',
      networkRegion: {
        wifi: 'connected',
        bluetooth: 'scanning'
      }
    });

    // muted_state tag removed, sound_playing tag added
    expect(snap.hasTag('muted_state')).toBe(false);
    expect(snap.hasTag('sound_playing')).toBe(true);
    // Ancestor audio tags retained
    expect(snap.hasTag('audio_subsystem')).toBe(true);
    expect(snap.hasTag('vol_control')).toBe(true);
    // Video and network tags intact
    expect(snap.hasTag('video_idle')).toBe(true);
    expect(snap.hasTag('wifi_online')).toBe(true);
    expect(snap.hasTag('bt_scan')).toBe(true);

    // Metadata: muted metadata removed, unmuted metadata added
    const meta = snap.getMeta();
    expect(meta.custom_audio_muted_id).toBeUndefined();
    expect(meta['deepParallelSystem.audioRegion.volumeControl.unmuted']).toEqual({
      volumeLevel: 50,
      state: 'audible'
    });
    // Video and network metadata intact
    expect(meta.custom_video_region_id).toEqual({ region: 'video' });
    expect(meta['deepParallelSystem.videoRegion.idle']).toEqual({ pipeline: 'stopped' });
    expect(meta['deepParallelSystem.networkRegion.wifi.connected']).toEqual({
      ssid: 'OfficeMesh',
      rssi: -45
    });
  });

  it('1.3 handles simultaneous transitions in multiple orthogonal regions', () => {
    const actor = createActor(createMachine(complexParallelConfig)).start();

    actor.send({ type: 'PLAY' });
    actor.send({ type: 'PAIR' });

    const snap = actor.getSnapshot();

    expect(snap.value).toEqual({
      audioRegion: { volumeControl: 'muted' },
      videoRegion: 'playing',
      networkRegion: {
        wifi: 'connected',
        bluetooth: 'paired'
      }
    });

    // Tags
    expect(snap.hasTag('video_idle')).toBe(false);
    expect(snap.hasTag('video_playing')).toBe(true);
    expect(snap.hasTag('hd_active')).toBe(true);
    expect(snap.hasTag('bt_scan')).toBe(false);
    expect(snap.hasTag('bt_connected')).toBe(true);

    // Meta
    const meta = snap.getMeta();
    expect(meta['deepParallelSystem.videoRegion.idle']).toBeUndefined();
    expect(meta['deepParallelSystem.videoRegion.playing']).toEqual({
      pipeline: 'streaming',
      resolution: '4K'
    });
    expect(meta['deepParallelSystem.networkRegion.bluetooth.scanning']).toBeUndefined();
    expect(meta.custom_bt_paired_id).toEqual({ deviceName: 'Headphones' });
  });

  it('1.4 verifies pure transitions (initialTransition and transition) match actor tags and metadata', () => {
    const machine = createMachine(complexParallelConfig);

    const [state0] = initialTransition(machine);
    const [state1] = transition(machine, state0, { type: 'UNMUTE' });
    const [state2] = transition(machine, state1, { type: 'PLAY' });

    expect(state2.value).toEqual({
      audioRegion: { volumeControl: 'unmuted' },
      videoRegion: 'playing',
      networkRegion: {
        wifi: 'connected',
        bluetooth: 'scanning'
      }
    });

    expect(state2.hasTag('sound_playing')).toBe(true);
    expect(state2.hasTag('video_playing')).toBe(true);
    expect(state2.hasTag('muted_state')).toBe(false);
    expect(state2.hasTag('video_idle')).toBe(false);

    const meta = state2.getMeta();
    expect(meta['deepParallelSystem.audioRegion.volumeControl.unmuted']).toEqual({
      volumeLevel: 50,
      state: 'audible'
    });
    expect(meta['deepParallelSystem.videoRegion.playing']).toEqual({
      pipeline: 'streaming',
      resolution: '4K'
    });
  });

  it('1.5 verifies Snapshot.tags Set operations and non-string tag query behavior', () => {
    const actor = createActor(createMachine(complexParallelConfig)).start();
    const snap = actor.getSnapshot();

    // Verify Set instance
    expect(snap.tags).toBeInstanceOf(Set);
    expect(snap.tags.size).toBe(12);

    // Verify Set.has vs hasTag equivalence for valid and absent strings
    for (const tag of [
      'system_active',
      'shared_tag',
      'audio_subsystem',
      'vol_control',
      'muted_state',
      'video_subsystem',
      'video_idle',
      'network_subsystem',
      'wifi_module',
      'wifi_online',
      'bt_module',
      'bt_scan',
      'non_existent_tag',
      '',
      'audio', // substring
      'wifi' // substring
    ]) {
      expect(snap.hasTag(tag)).toBe(snap.tags.has(tag));
    }

    // Verify Set methods safely return false for non-string queries
    expect(snap.tags.has(null as any)).toBe(false);
    expect(snap.tags.has(undefined as any)).toBe(false);
    expect(snap.tags.has(123 as any)).toBe(false);
    expect(snap.tags.has({} as any)).toBe(false);

    // Verify spread and iteration
    const tagArray = [...snap.tags];
    expect(tagArray.length).toBe(12);
  });
});
