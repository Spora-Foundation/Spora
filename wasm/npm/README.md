# Tondi WASM SDK

An integration wrapper around [`tondi-wasm`](https://www.npmjs.com/package/tondi-wasm) module that uses [`websocket`](https://www.npmjs.com/package/websocket) W3C adaptor for WebSocket communication.

This is a Node.js module that provides bindings to the Tondi WASM SDK strictly for use in the Node.js environment. The web browser version of the SDK is available as part of official SDK releases at [https://github.com/AvatoLabs/Tondi/releases](https://github.com/AvatoLabs/Tondi/releases)

## Usage

Tondi NPM module exports include all WASM32 bindings.
```javascript
const tondi = require('tondi');
console.log(tondi.version());
```

## Documentation

Documentation is available at [https://tondi.aspectron.org/docs/](https://tondi.aspectron.org/docs/)


## Building from source & Examples

SDK examples as well as information on building the project from source can be found at [https://github.com/AvatoLabs/Tondi/tree/master/wasm](https://github.com/AvatoLabs/Tondi/tree/master/wasm)

## Releases

Official releases as well as releases for Web Browsers are available at [https://github.com/AvatoLabs/Tondi/releases](https://github.com/AvatoLabs/Tondi/releases).

Nightly / developer builds are available at: [https://aspectron.org/en/projects/tondi-wasm.html](https://aspectron.org/en/projects/tondi-wasm.html)

