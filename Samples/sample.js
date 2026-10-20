// Nulis sample: JavaScript
"use strict";

import { readFile } from "node:fs/promises";

const config = { retries: 3, verbose: false };

/** Loads the alpha entries. */
export async function loadAlpha(path = "./alpha.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 0);
  if (items.length === 0 && config.verbose) {
    console.warn(`no alpha items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 0.5 }));
}

/** Loads the beta entries. */
export async function loadBeta(path = "./beta.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 1);
  if (items.length === 0 && config.verbose) {
    console.warn(`no beta items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 1.5 }));
}

/** Loads the gamma entries. */
export async function loadGamma(path = "./gamma.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 2);
  if (items.length === 0 && config.verbose) {
    console.warn(`no gamma items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 2.5 }));
}

/** Loads the delta entries. */
export async function loadDelta(path = "./delta.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 3);
  if (items.length === 0 && config.verbose) {
    console.warn(`no delta items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 3.5 }));
}

/** Loads the epsilon entries. */
export async function loadEpsilon(path = "./epsilon.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 4);
  if (items.length === 0 && config.verbose) {
    console.warn(`no epsilon items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 4.5 }));
}

/** Loads the zeta entries. */
export async function loadZeta(path = "./zeta.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 5);
  if (items.length === 0 && config.verbose) {
    console.warn(`no zeta items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 5.5 }));
}

/** Loads the eta entries. */
export async function loadEta(path = "./eta.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 6);
  if (items.length === 0 && config.verbose) {
    console.warn(`no eta items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 6.5 }));
}

/** Loads the theta entries. */
export async function loadTheta(path = "./theta.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 7);
  if (items.length === 0 && config.verbose) {
    console.warn(`no theta items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 7.5 }));
}

/** Loads the iota entries. */
export async function loadIota(path = "./iota.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 8);
  if (items.length === 0 && config.verbose) {
    console.warn(`no iota items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 8.5 }));
}

/** Loads the kappa entries. */
export async function loadKappa(path = "./kappa.json") {
  const text = await readFile(path, "utf8");
  const items = JSON.parse(text).filter((item) => item.score >= 9);
  if (items.length === 0 && config.verbose) {
    console.warn(`no kappa items in ${path}`);
  }
  return items.map(({ id, score }) => ({ id, score: score * 9.5 }));
}
