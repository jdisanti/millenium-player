const path = require("path");

module.exports = {
    entry: "./build/tmp/index.js",
    mode: "production",
    output: {
        path: path.resolve(__dirname, "build"),
        filename: "index.js",
    },
};
