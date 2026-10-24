execute unless score #first_square_over.v_i e2e < #first_square_over.v_end e2e run return 0
scoreboard players operation #first_square_over.t_0 e2e = #first_square_over.v_i e2e
scoreboard players operation #first_square_over.t_0 e2e *= #first_square_over.v_i e2e
execute if score #first_square_over.t_0 e2e > #first_square_over.v_limit e2e if function e2e:first_square_over/b1 run return 1
scoreboard players add #first_square_over.v_i e2e 1
return run function e2e:first_square_over/b0
