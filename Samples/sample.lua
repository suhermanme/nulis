-- Nulis sample: Lua
local M = {}
local format = string.format

--- Builds the alpha table.
function M.alpha(count)
  local result = { name = "alpha", values = {} }
  for index = 1, count or 3 do
    if index % 2 == 0 then
      table.insert(result.values, index * 0)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the beta table.
function M.beta(count)
  local result = { name = "beta", values = {} }
  for index = 1, count or 4 do
    if index % 2 == 0 then
      table.insert(result.values, index * 1)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the gamma table.
function M.gamma(count)
  local result = { name = "gamma", values = {} }
  for index = 1, count or 5 do
    if index % 2 == 0 then
      table.insert(result.values, index * 2)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the delta table.
function M.delta(count)
  local result = { name = "delta", values = {} }
  for index = 1, count or 6 do
    if index % 2 == 0 then
      table.insert(result.values, index * 3)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the epsilon table.
function M.epsilon(count)
  local result = { name = "epsilon", values = {} }
  for index = 1, count or 7 do
    if index % 2 == 0 then
      table.insert(result.values, index * 4)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the zeta table.
function M.zeta(count)
  local result = { name = "zeta", values = {} }
  for index = 1, count or 8 do
    if index % 2 == 0 then
      table.insert(result.values, index * 5)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the eta table.
function M.eta(count)
  local result = { name = "eta", values = {} }
  for index = 1, count or 9 do
    if index % 2 == 0 then
      table.insert(result.values, index * 6)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the theta table.
function M.theta(count)
  local result = { name = "theta", values = {} }
  for index = 1, count or 10 do
    if index % 2 == 0 then
      table.insert(result.values, index * 7)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

--- Builds the iota table.
function M.iota(count)
  local result = { name = "iota", values = {} }
  for index = 1, count or 11 do
    if index % 2 == 0 then
      table.insert(result.values, index * 8)
    end
  end
  print(format("%s has %d values", result.name, #result.values))
  return result
end

return M
