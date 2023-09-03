// This file is part of Millenium Player.
// Copyright (C) 2023 John DiSanti.
//
// Millenium Player is free software: you can redistribute it and/or modify it under the terms of
// the GNU General Public License as published by the Free Software Foundation, either version 3 of
// the License, or (at your option) any later version.
//
// Millenium Player is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with Millenium Player.
// If not, see <https://www.gnu.org/licenses/>.

import Handlebars from "handlebars";
import * as process from "process";
import * as fs from "fs";

const target_os = (() => {
    switch (process.platform) {
        case "win32":
            return "windows";
        case "darwin":
            return "macos";
    }
    return "linux";
})();

const webview_background_color = (() => {
    if (target_os == "windows") {
        return "rgba(0,0,0,1)";
    }
    return "rgba(0,0,0,0)";
})();

const data = {
    webview_background_color,
    target_os,
};

const files = [
    { input_name: "src/index.html.hbs", output_name: "build/index.html" },
];
for (const file of files) {
    const template_contents = fs.readFileSync(file.input_name, "utf8");
    console.log(
        `rendering ${file.input_name} to ${
            file.output_name
        } with ${JSON.stringify(data)}`,
    );
    fs.writeFileSync(
        file.output_name,
        Handlebars.compile(template_contents)(data),
    );
}
