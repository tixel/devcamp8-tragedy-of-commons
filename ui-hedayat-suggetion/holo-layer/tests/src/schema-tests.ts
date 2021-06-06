const sleep = (ms) =>
  new Promise((resolve) => setTimeout(() => resolve(null), ms));
import path from "path";
import { InstallAgentApp, _log, Peershare_Zome } from "./common";
import fs from "fs";

// These zome functions are going to be tested in this scenario.
const enum zome_functions {
  Create_Schema = "create_schema",
  Get_Schema_Address = "get_schema_address",
  Get_All_Schemas = "get_all_schemas",
}

module.exports = async (orchestrator) => {
  orchestrator.registerScenario("upload schema-jsaon!", async (s, t) => {
    const alice_cell = await InstallAgentApp(
      s,
      "alice-cell-upload-schema",
      true
    );

    //*********** Test Case: create_schema, Success
    const schema = {
      definition: "fake data",
      version: "v1",
    };
    let create_schema_result_alice = await alice_cell.call(
      Peershare_Zome,
      zome_functions.Create_Schema,
      schema
    );
    _log("Create_Schema_Result", create_schema_result_alice.toString("base64"));
    t.ok(create_schema_result_alice);

    await sleep(10);

    //*********** Test Case: Get Element

    let element_result = await alice_cell.call(
      Peershare_Zome,
      "get_schema_element",
      schema
    );
    _log("Element", element_result);
    t.ok(element_result);

    await sleep(10);
    //*********** Test Case: create_schema, Faild becuase non-developer tried to create schema
    const bob_cell = await InstallAgentApp(s, "bob-cell-upload-schema", false);
    try {
      let create_schema_result_bob = await bob_cell.call(
        Peershare_Zome,
        zome_functions.Create_Schema,
        {
          definition: schema,
          version: "v1",
        }
      );
      t.fail();
    } catch (e) {
      // t.deepEqual(e, {
      //   type: "error",
      //   data: {
      //     type: "ribosome_error",
      //     data: `Wasm error while working with Ribosome: Guest("You are not the developer, so you can\\'t create a schema")`,
      //   },
      // });
      _log("e", e);
      t.deepEqual(e.type, "error");
    }
  });
};
