import {
  Orchestrator,
  Config,
  InstallAgentsHapps,
  TransportConfigType,
  Player,
} from "@holochain/tryorama";
import { ScenarioApi } from "@holochain/tryorama/lib/api";
import path from "path";

const conductorConfig = Config.gen({});

// Construct proper paths for your DNAs
const dnaPath = path.join(__dirname, "../../workdir/dna/sample.dna");

const sleep = (ms) => new Promise((resolve) => setTimeout(() => resolve(), ms));

const orchestrator = new Orchestrator();

// create an InstallAgentsHapps array with your DNAs to tell tryorama what
// to install into the conductor.
const installation: InstallAgentsHapps = [
  // agent 0
  [
    // happ 0
    [dnaPath],
  ],
    // agent 1
  [
    // happ 0
    [dnaPath],
  ],
];

// orchestrator.registerScenario(
//   "create and get a calendar event",
//   async (s, t) => {
//     const [player]: Player[] = await s.players([conductorConfig]);
//     const [[alice_happ], [bob_happ]] = await player.installAgentsHapps(
//       installation
//     );

//     const alice_calendar = alice_happ.cells[0];
//     const bob_calendar = bob_happ.cells[0];

//     let calendarEvent = await alice_calendar.call(
//       "tragedy_of_commons",
//       "create_calendar_event",
//       {
//         title: "Event 1",
//         start_time: [Math.floor(Date.now() / 1000), 0],
//         end_time: [Math.floor(Date.now() / 1000) + 1000, 0],
//         location: { Custom: "hiii" },
//         invitees: [],
//       }
//     );
//     t.ok(calendarEvent);

//     await sleep(10);

//     let calendarEvents = await alice_calendar.call(
//       "tragedy_of_commons",
//       "get_all_calendar_events",
//       null
//     );
//     t.equal(calendarEvents.length, 1);

//   }
// );

orchestrator.registerScenario(
  "start new game session with 2 players",
  async (s: ScenarioApi, t) => {
    // SETUP
    const ZOME_NAME = "tragedy_of_commons";
    const [alice, bob] = await s.players([conductorConfig, conductorConfig]);
    const [[alice_common]] = await alice.installAgentsHapps(installation);
    const [[bob_common]] = await bob.installAgentsHapps(installation);

    await s.shareAllNodes([alice, bob])


    // SIGNAL HANDLERS
    let prev_round_hash;
    let signalPromiseAlice = new Promise<void>((resolve) => alice.setSignalHandler((signal) => {
      let payload = signal.data.payload
      t.ok(payload);
      console.log("Alice received Signal:", signal.data.payload);
      prev_round_hash = signal.data.payload.signal_payload.previous_round_entry_hash;
      resolve();
    }));
    // .then(function(data) {
    //     tixel: there should be a way to write this test without the sleep(5000) workaround
    //            using chained promises, but I haven't figured out how
    //     console.log("INSIDE PROMISE:",data);
    //     return null;    
    // });
    let signalPromiseBob = new Promise<void>((resolve) => bob.setSignalHandler((signal) => {
      let payload = signal.data.payload
      t.ok(payload);
      console.log("Bob received Signal:");
      resolve();
    }));


    // START GAME
    //Alice starts a new game (session) with bob and herself
    let session_header_hash = await alice_common.cells[0].call(
      ZOME_NAME,
      "start_dummy_session",
      [bob_common.agent, alice_common.agent]
    );
    console.log(session_header_hash);
    t.ok(session_header_hash);

    //Ensure every thing is ok
    await signalPromiseAlice;
    await signalPromiseBob;

    sleep(500);
    console.log("prev_round_hash", prev_round_hash);

    // ROUND 1
    // Alice makes 1 move
    let game_move_round_1_alice = await alice_common.cells[0].call(
      ZOME_NAME,
      "new_move",
      {resource_amount: 5, previous_round: prev_round_hash},
    );

    // Bob makes 1 move
    let game_move_round_1_bob = await bob_common.cells[0].call(
      ZOME_NAME,
      "new_move",
      {resource_amount: 125, previous_round: prev_round_hash},
    );
    console.log(game_move_round_1_alice);
    t.ok(game_move_round_1_alice);
    console.log(game_move_round_1_bob);
    t.ok(game_move_round_1_bob);

    // CHECK  TO CLOSE GAME
    let close_game_round_1_bob = await bob_common.cells[0].call(
      ZOME_NAME,
      "try_to_close_round",
      prev_round_hash,
    );
  }
);

orchestrator.run();
