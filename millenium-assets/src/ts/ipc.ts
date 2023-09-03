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

export type Direction = "to_rust" | "from_rust";

export type MessageHandler = (msg: Message) => void;
export type MessageHanderId = number;

export class Message {
    private static next_handler_id: MessageHanderId = 0;

    constructor(
        public direction: Direction,
        public kind: string,
        public data: any,
    ) {}

    static from_json(json: string): Message {
        const obj = JSON.parse(json);
        if (obj.direction || obj.type == undefined || obj.data == undefined) {
            throw new Error(`invalid message: ${json}`);
        }
        return new Message(obj.direction, obj.kind, obj.data);
    }

    private to_json(): string {
        return JSON.stringify(this);
    }

    static send(kind: string, data: any) {
        const ipc: any = (window as any)["ipc"];
        ipc.postMessage(new Message("to_rust", kind, data).to_json());
    }

    private static handlers: {
        id: MessageHanderId;
        handler: MessageHandler;
    }[] = [];
    static push_message_handler(handler: MessageHandler): MessageHanderId {
        Message.next_handler_id += 1;
        Message.handlers.push({ id: Message.next_handler_id, handler });
        return Message.next_handler_id;
    }
    static remove_message_handler(id: MessageHanderId) {
        Message.handlers = Message.handlers.filter(
            (handler) => handler.id != id,
        );
    }
    static handle(kind: string, data: any) {
        for (const { handler } of Message.handlers) {
            handler(new Message("from_rust", kind, data));
        }
    }
}

export interface UiData {
    waveform: {
        spectrum: number[];
        amplitude: number[];
    };
}

export class IpcFetchInterval {
    private interval_id: any | null = null;
    private fetching: boolean = false;
    private fetch_success: (response: Response) => Promise<void> | void =
        () => {};
    private fetch_failure: (err: any) => Promise<void> | void = () => {};

    constructor(
        private interval_millis: number,
        private url: string,
    ) {}

    fetch_now() {
        if (!this.fetching) {
            this.fetching = true;
            fetch(this.url)
                .then((response) => {
                    this.fetching = false;
                    return this.fetch_success(response);
                })
                .catch((err) => {
                    this.fetching = false;
                    return this.fetch_failure(err);
                });
        }
    }

    start(): IpcFetchInterval {
        if (this.interval_id == null) {
            this.fetch_now();
            this.interval_id = setInterval(() => {
                this.fetch_now();
            }, this.interval_millis);
        }
        return this;
    }

    stop(): IpcFetchInterval {
        if (this.interval_id != null) {
            clearInterval(this.interval_id);
            this.interval_id = null;
        }
        return this;
    }

    on_success(callback: (response: Response) => void): IpcFetchInterval {
        this.fetch_success = callback;
        return this;
    }

    on_failure(callback: (err: any) => void): IpcFetchInterval {
        this.fetch_failure = callback;
        return this;
    }
}
