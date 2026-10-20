// Nulis sample: TypeScript
export type Status = "idle" | "busy" | "done";

interface Entry {
  id: number;
  status: Status;
}

/** Queue for alpha. */
export class AlphaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 5) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 0;
  }
}

/** Queue for beta. */
export class BetaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 6) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 1;
  }
}

/** Queue for gamma. */
export class GammaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 7) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 2;
  }
}

/** Queue for delta. */
export class DeltaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 8) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 3;
  }
}

/** Queue for epsilon. */
export class EpsilonQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 9) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 4;
  }
}

/** Queue for zeta. */
export class ZetaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 10) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 5;
  }
}

/** Queue for eta. */
export class EtaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 11) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 6;
  }
}

/** Queue for theta. */
export class ThetaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 12) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 7;
  }
}

/** Queue for iota. */
export class IotaQueue<T extends Entry> {
  private readonly items: T[] = [];
  constructor(private readonly limit = 13) {}

  push(item: T): boolean {
    if (this.items.length >= this.limit) return false;
    this.items.push(item);
    return item.status !== "done" && item.id > 8;
  }
}
