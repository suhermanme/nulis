-- Nulis sample: Oracle SQL / PL/SQL
SET SERVEROUTPUT ON;

-- Procedure 0: alpha
CREATE OR REPLACE PROCEDURE update_alpha (p_limit IN NUMBER DEFAULT 0) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'ALPHA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for alpha');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_alpha;
/

-- Procedure 1: beta
CREATE OR REPLACE PROCEDURE update_beta (p_limit IN NUMBER DEFAULT 1) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'BETA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for beta');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_beta;
/

-- Procedure 2: gamma
CREATE OR REPLACE PROCEDURE update_gamma (p_limit IN NUMBER DEFAULT 2) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'GAMMA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for gamma');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_gamma;
/

-- Procedure 3: delta
CREATE OR REPLACE PROCEDURE update_delta (p_limit IN NUMBER DEFAULT 3) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'DELTA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for delta');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_delta;
/

-- Procedure 4: epsilon
CREATE OR REPLACE PROCEDURE update_epsilon (p_limit IN NUMBER DEFAULT 4) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'EPSILON' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for epsilon');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_epsilon;
/

-- Procedure 5: zeta
CREATE OR REPLACE PROCEDURE update_zeta (p_limit IN NUMBER DEFAULT 5) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'ZETA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for zeta');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_zeta;
/

-- Procedure 6: eta
CREATE OR REPLACE PROCEDURE update_eta (p_limit IN NUMBER DEFAULT 6) AS
  v_count NUMBER := 0;
BEGIN
  FOR rec IN (SELECT id, amount FROM orders WHERE amount > p_limit) LOOP
    UPDATE orders SET status = 'ETA' WHERE id = rec.id;
    v_count := v_count + 1;
  END LOOP;
  DBMS_OUTPUT.PUT_LINE('updated ' || v_count || ' rows for eta');
  COMMIT;
EXCEPTION
  WHEN OTHERS THEN ROLLBACK; RAISE;
END update_eta;
/
