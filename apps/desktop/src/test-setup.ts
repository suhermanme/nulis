import { configure } from "@testing-library/react";

// Shared CI machines are several times slower than a developer's. The default
// one-second wait for asynchronous updates is too tight there and makes
// otherwise correct tests fail at random, so waits get more room. A passing
// wait still returns as soon as its condition holds.
configure({ asyncUtilTimeout: 5000 });
