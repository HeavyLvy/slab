scoreboard players set #first_square_over.v_i e2e 0
scoreboard players set #first_square_over.v_end e2e 1000
execute if function e2e:first_square_over/b0 run return 1
scoreboard players set #ret e2e -1
return 1
