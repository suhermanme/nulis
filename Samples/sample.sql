-- Nulis sample: SQL
/* Standard SQL queries */

-- Query 0: alpha
CREATE TABLE alpha (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 0.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO alpha (id, name, amount) VALUES (0, 'alpha', 0.25);
SELECT name, SUM(amount) AS total
FROM alpha
WHERE active = TRUE AND amount > 0
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 1: beta
CREATE TABLE beta (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 1.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO beta (id, name, amount) VALUES (1, 'beta', 3.25);
SELECT name, SUM(amount) AS total
FROM beta
WHERE active = TRUE AND amount > 1
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 2: gamma
CREATE TABLE gamma (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 2.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO gamma (id, name, amount) VALUES (2, 'gamma', 6.25);
SELECT name, SUM(amount) AS total
FROM gamma
WHERE active = TRUE AND amount > 2
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 3: delta
CREATE TABLE delta (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 3.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO delta (id, name, amount) VALUES (3, 'delta', 9.25);
SELECT name, SUM(amount) AS total
FROM delta
WHERE active = TRUE AND amount > 3
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 4: epsilon
CREATE TABLE epsilon (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 4.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO epsilon (id, name, amount) VALUES (4, 'epsilon', 12.25);
SELECT name, SUM(amount) AS total
FROM epsilon
WHERE active = TRUE AND amount > 4
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 5: zeta
CREATE TABLE zeta (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 5.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO zeta (id, name, amount) VALUES (5, 'zeta', 15.25);
SELECT name, SUM(amount) AS total
FROM zeta
WHERE active = TRUE AND amount > 5
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 6: eta
CREATE TABLE eta (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 6.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO eta (id, name, amount) VALUES (6, 'eta', 18.25);
SELECT name, SUM(amount) AS total
FROM eta
WHERE active = TRUE AND amount > 6
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;

-- Query 7: theta
CREATE TABLE theta (
    id INTEGER PRIMARY KEY,
    name VARCHAR(80) NOT NULL,
    amount NUMERIC(10, 2) DEFAULT 7.00,
    active BOOLEAN DEFAULT TRUE
);
INSERT INTO theta (id, name, amount) VALUES (7, 'theta', 21.25);
SELECT name, SUM(amount) AS total
FROM theta
WHERE active = TRUE AND amount > 7
GROUP BY name HAVING COUNT(*) > 1 ORDER BY total DESC;
