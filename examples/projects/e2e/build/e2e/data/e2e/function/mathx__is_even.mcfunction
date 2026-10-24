execute if score #mathx__is_even.v_n e2e matches 0 if function e2e:mathx__is_even/b0 run return 1
scoreboard players operation #mathx__is_even.t_0 e2e = #mathx__is_even.v_n e2e
scoreboard players remove #mathx__is_even.t_0 e2e 1
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #mathx__is_even.v_n e2e
scoreboard players operation #mathx__is_odd.v_n e2e = #mathx__is_even.t_0 e2e
function e2e:mathx__is_odd
execute store result score #mathx__is_even.v_n e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
return 1
