local MAX_EXACT = 4503599627370495

local function is_integer(value)
    return value ~= nil
        and value >= 0
        and value <= MAX_EXACT
        and value == math.floor(value)
end

local function ceil_div(value, divisor)
    if value == 0 then
        return 0
    end

    return math.floor((value - 1) / divisor) + 1
end

local function decimal(value)
    return string.format("%.0f", value)
end

if #KEYS ~= 1 or #ARGV ~= 3 then
    return redis.error_reply("ERR expected one key and three arguments")
end

local key = KEYS[1]
local max_units = tonumber(ARGV[1])
local units_per_token = tonumber(ARGV[2])
local refill_rate = tonumber(ARGV[3])

if not is_integer(max_units) or max_units == 0 then
    return redis.error_reply("ERR invalid maximum token units")
end

if not is_integer(units_per_token) or units_per_token == 0 then
    return redis.error_reply("ERR invalid units per token")
end

if not is_integer(refill_rate) or refill_rate == 0 then
    return redis.error_reply("ERR invalid refill rate")
end

if max_units < units_per_token
    or max_units % units_per_token ~= 0 then
    return redis.error_reply("ERR invalid bucket capacity")
end

local time = redis.call("TIME")
local now_us = tonumber(time[1]) * 1000000 + tonumber(time[2])

if not is_integer(now_us) then
    return redis.error_reply("ERR server time exceeds supported range")
end

local token_units = max_units
local updated_at_us = now_us

if redis.call("EXISTS", key) == 1 then
    local state = redis.call(
        "HMGET", key,
        "token_units",
        "updated_at_us",
        "max_units",
        "units_per_token",
        "refill_rate"
    )

    local values = {}

    for index = 1, 5 do
        local value = tonumber(state[index] or "")

        if not is_integer(value) then
            return redis.error_reply(
                "PACEKEEPER_INVALID_STATE missing or invalid bucket field"
            )
        end

        values[index] = value
    end

    if values[3] ~= max_units
        or values[4] ~= units_per_token
        or values[5] ~= refill_rate then
        return redis.error_reply(
            "PACEKEEPER_CONFIG_MISMATCH bucket configuration differs"
        )
    end

    token_units = values[1]
    updated_at_us = values[2]

    if token_units > max_units then
        return redis.error_reply(
            "PACEKEEPER_INVALID_STATE tokens exceed capacity"
        )
    end

    if now_us < updated_at_us then
        return redis.error_reply(
            "PACEKEEPER_CLOCK_BACKWARDS server time moved backwards"
        )
    end
end

local elapsed_us = now_us - updated_at_us
local missing_units = max_units - token_units
local time_until_full_us = ceil_div(missing_units, refill_rate)

if elapsed_us >= time_until_full_us then
    token_units = max_units
else
    token_units = token_units + elapsed_us * refill_rate
end

local status = 0
local wait_us

if token_units >= units_per_token then
    token_units = token_units - units_per_token
    status = 1

    wait_us = ceil_div(max_units - token_units, refill_rate)
else
    wait_us = ceil_div(units_per_token - token_units, refill_rate)
end

local remaining = math.floor(token_units / units_per_token)

redis.call(
    "HSET", key,
    "token_units", decimal(token_units),
    "updated_at_us", decimal(now_us),
    "max_units", decimal(max_units),
    "units_per_token", decimal(units_per_token),
    "refill_rate", decimal(refill_rate)
)

local full_after_us = ceil_div(max_units - token_units, refill_rate)
local ttl_ms = ceil_div(full_after_us, 1000) + 1

redis.call("PEXPIRE", key, decimal(ttl_ms))

return {status, remaining, wait_us}