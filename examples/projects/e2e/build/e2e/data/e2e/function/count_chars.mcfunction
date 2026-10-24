execute if score #count_chars.v_n e2e matches 0 if function e2e:count_chars/b0 run return 1
execute store result score #count_chars.v_before e2e run data get storage e2e:vars count_chars.v_label
scoreboard players operation #count_chars.t_0 e2e = #count_chars.v_n e2e
scoreboard players remove #count_chars.t_0 e2e 1
data modify storage e2e:vars count_chars.t_0 set from storage e2e:vars count_chars.v_label
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #count_chars.v_n e2e
data modify storage e2e:stack d append value 0
data modify storage e2e:stack d[-1] set from storage e2e:vars count_chars.v_label
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #count_chars.v_before e2e
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #count_chars.v_rest e2e
scoreboard players operation #count_chars.v_n e2e = #count_chars.t_0 e2e
data modify storage e2e:vars count_chars.v_label set from storage e2e:vars count_chars.t_0
data remove storage e2e:vars count_chars.t_0
function e2e:count_chars
execute store result score #count_chars.v_rest e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
execute store result score #count_chars.v_before e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
data modify storage e2e:vars count_chars.v_label set from storage e2e:stack d[-1]
data remove storage e2e:stack d[-1]
execute store result score #count_chars.v_n e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
scoreboard players operation #count_chars.v_rest e2e = #ret e2e
scoreboard players operation #ret e2e = #count_chars.v_rest e2e
scoreboard players operation #ret e2e += #count_chars.v_before e2e
data remove storage e2e:vars count_chars.v_label
return 1
