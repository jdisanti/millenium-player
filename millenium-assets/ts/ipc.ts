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

export class Message {
    constructor(public kind: string, public data: any) { }

    static from_json(json: string): Message {
        let obj = JSON.parse(json);
        if (obj.type == undefined || obj.data == undefined) {
            throw new Error(`invalid message: ${json}`);
        }
        return new Message(obj.kind, obj.data);
    }

    private to_json(): string {
        return JSON.stringify(this);
    }

    static send(kind: string, data: any) {
        const ipc: any = (window as any)["ipc"];
        ipc.postMessage(new Message(kind, data).to_json());
    }
}
