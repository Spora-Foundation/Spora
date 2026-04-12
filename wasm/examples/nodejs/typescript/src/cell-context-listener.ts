// Run with: node demo.js
import { w3cwebsocket } from "websocket";
(globalThis.WebSocket as any) = w3cwebsocket;

import {
    PrivateKey,
    Address,
    RpcClient,
    Resolver,
    CellProcessor,
    CellContext,
    sporaToSau,
    createTransactions,
    initConsolePanicHook,
    IPendingEvent,
    IDiscoveryEvent,
    IMaturityEvent,
    ITransactionRecord,
    CellProcessorEventType,
} from "../../../../nodejs/spora";

import { parseArgs } from "./utils";


initConsolePanicHook();


let { encoding, networkId, destinationAddress } = parseArgs();

(async () => {

    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    let address = new Address("spora:qrxkessyzxkv5ve7rj7u36nxxvtt08lsknnd8e8zw3p7xsf8ck0cqeuyrhkp0")
    address = new Address("spora:qpamkvhgh0kzx50gwvvp5xs8ktmqutcy3dfs9dc3w7lm9rq0zs76vf959mmrp");
    console.info(`Source address: ${sourceAddress}`);
    console.info(`address: ${address}`);

    // if not destination is specified, send back to ourselves
    destinationAddress = destinationAddress ?? sourceAddress;
    console.info(`Destination address: ${destinationAddress}`);

    // 1) Initialize RPC
    const rpc = new RpcClient({
        resolver: new Resolver(),
        encoding,
        networkId
    });

    // 2) Create CellProcessor, passing RPC to it
    let processor = new CellProcessor({ rpc, networkId });
    await processor.start();

    // 3) Create one or more CellContext instances, passing CellProcessor to them
    // you can create CellContext objects as needed to monitor different
    // address sets.
    let context = new CellContext({ processor });

    // 4) Register a listener with the CellProcessor::events
    processor.addEventListener(({event, data}) => {
        // handle the event
        // since the event is a union type, you can switch on the 
        // event type and cast the data to the appropriate type
        switch (event) {
            //string enums are not working for @Matoo
            //case CellProcessorEventType.Discovery: {
            case "discovery": {
                let record = data.record;
                console.log("Discovery event record:", record);
            } break;
            //case CellProcessorEventType.Pending: {
            case "pending":{
                let record = data.record;
                console.log("Pending event record:", record);
            } break;
            //case CellProcessorEventType.Maturity: {
            case "maturity": {
                let record = data.record;
                console.log("Maturity event record:", record);
            } break;
            default: {
                console.log("Other event:", event, "data:", data);
            }
        }
    });

    processor.addEventListener("discovery", (data) => {
        console.log("Discovery event record:", data.record);
    });

    console.log(processor);

    // 5) Once the environment is setup, connect to RPC
    console.log(`Connecting to ${rpc.url}`);
    await rpc.connect();

    // for local nodes, wait for the node to sync
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    // 6) Register the address list with the CellContext
    await context.trackAddresses([sourceAddress], undefined);

})();
