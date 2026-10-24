execute if score #mathx__fib.v_n e2e matches ..1 if function e2e:mathx__fib/b0 run return 1
scoreboard players operation #mathx__fib.t_0 e2e = #mathx__fib.v_n e2e
scoreboard players remove #mathx__fib.t_0 e2e 1
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #mathx__fib.v_n e2e
scoreboard players operation #mathx__fib.v_n e2e = #mathx__fib.t_0 e2e
function e2e:mathx__fib
execute store result score #mathx__fib.v_n e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
scoreboard players operation #mathx__fib.t_0 e2e = #ret e2e
scoreboard players operation #mathx__fib.t_1 e2e = #mathx__fib.v_n e2e
scoreboard players remove #mathx__fib.t_1 e2e 2
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #mathx__fib.v_n e2e
data modify storage e2e:stack i append value 0
execute store result storage e2e:stack i[-1] int 1 run scoreboard players get #mathx__fib.t_0 e2e
scoreboard players operation #mathx__fib.v_n e2e = #mathx__fib.t_1 e2e
function e2e:mathx__fib
execute store result score #mathx__fib.t_0 e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
execute store result score #mathx__fib.v_n e2e run data get storage e2e:stack i[-1]
data remove storage e2e:stack i[-1]
scoreboard players operation #mathx__fib.t_1 e2e = #ret e2e
scoreboard players operation #ret e2e = #mathx__fib.t_0 e2e
scoreboard players operation #ret e2e += #mathx__fib.t_1 e2e
return 1
