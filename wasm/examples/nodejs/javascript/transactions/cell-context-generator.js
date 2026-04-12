// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    Generator,
    CellProcessor,
    CellContext,
    sporaToSau,
    createTransactions,
    initConsolePanicHook
} = require('../../../../nodejs/spora');

initConsolePanicHook();

const { encoding, networkId, address : destinationAddress } = require("../utils").parseArgs();


(async () => {

    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    console.log(`Source address: ${sourceAddress}`);

    // if not destination is specified, send back to ourselves
    let address = destinationAddress ?? sourceAddress;
    console.log(`Destination address: ${destinationAddress}`);

    // 1) Initialize RPC
    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });

    // 2) Create CellProcessor, passing RPC to it
    let processor = new CellProcessor({ rpc, networkId });
    await processor.start();

    // 3) Create one or more CellContext instances, passing CellProcessor to them
    // you can create CellContext objects as needed to monitor different
    // address sets.
    let context = await new CellContext({ processor });

    // 4) Register a listener with the CellProcessor::events
    processor.addEventListener((event) => {
        console.log("event:", event);
    });

    console.log(processor);

    // 5) Once the environment is setup, connect to RPC
    console.log(`Connecting to ${rpc.url}`);
    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    // 6) Register the address list with the CellContext
    await context.trackAddresses([sourceAddress]);

    // 7) Check balance, if there are enough funds, send a transaction
    if (context.balance.mature > sporaToSau(0.2) + 1000n) {
        console.log("Sending transaction");

        let generator = new Generator({
            entries : context,
            outputs: [{address, amount : sporaToSau(0.2)}],
            priorityFee: sporaToSau(0.0001),
            changeAddress: sourceAddress,
        });

        let pending;
        while (pending = await generator.next()) {
            await pending.sign([privateKey]);
            let txid = await pending.submit(rpc);
            console.log("txid:", txid);
        }

        console.log("summary:", generator.summary());

    } else {
        console.log("Not enough funds to send transaction");
    }

    await processor.shutdown();
    await rpc.disconnect();

})();
